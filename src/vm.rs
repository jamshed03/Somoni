use std::rc::Rc;

use crate::ast::BinaryOp;
use crate::bytecode::{Chunk, Op, Path, PathStep, Program};
use crate::error::{ErrorCode, SomoniError};
use crate::map::MapValue;
use crate::text::Text;
use crate::value::{self, Object, Step, StructValue, Value, map_key, show_key};

/// More nested calls than this are runtime error R004.
const MAX_CALL_DEPTH: usize = 10_000;

/// Runs a compiled program.
pub fn run(program: &Program) -> Result<(), SomoniError> {
    Vm::default().run(program)
}

/// One running chunk: the program or a called function.
/// `ip` is the next instruction, `base` the place of its first variable on the stack.
struct Frame<'p> {
    chunk: &'p Chunk,
    ip: usize,
    base: usize,
}

/// The virtual machine: a loop over the instructions, and the frames of the callers.
/// The stack holds the variables of each running function, starting at its `base`,
/// and above them the values the instructions work on.
#[derive(Default)]
struct Vm<'p> {
    stack: Vec<Value>,
    callers: Vec<Frame<'p>>,
}

impl<'p> Vm<'p> {
    /// Runs a program, starting with its main chunk.
    fn run(&mut self, program: &'p Program) -> Result<(), SomoniError> {
        let mut frame = Frame {
            chunk: &program.main,
            ip: 0,
            base: 0,
        };
        self.stack.resize(frame.chunk.locals, Value::Void);
        while let Some(op) = frame.chunk.code.get(frame.ip) {
            let (chunk, at) = (frame.chunk, frame.ip);
            frame.ip += 1;
            self.execute(op, &mut frame, program)
                .map_err(|error| error.with_span_if_missing(Some(chunk.spans[at])))?;
        }
        Ok(())
    }

    /// Runs one instruction of `frame`. Calls and returns change `frame` to another chunk.
    fn execute(
        &mut self,
        op: &Op,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<(), SomoniError> {
        match op {
            Op::Push(i) => self.stack.push(frame.chunk.constants[*i].clone()),
            Op::GetLocal(slot) => self.stack.push(self.stack[frame.base + slot].clone()),
            Op::SetLocal(slot) => {
                let value = self.pop()?;
                self.stack[frame.base + slot] = value;
            }
            Op::Pop => {
                self.pop()?;
            }
            Op::Binary(op) => {
                let right = self.pop()?;
                let left = self.pop()?;
                self.stack.push(value::binary(*op, left, right)?);
            }
            Op::Unary(op) => {
                let operand = self.pop()?;
                self.stack.push(value::unary(*op, operand)?);
            }
            Op::CallBuiltin(builtin, count) => {
                let start = self.stack.len() - count;
                let result = builtin.call(&self.stack[start..])?;
                self.stack.truncate(start);
                self.stack.push(result);
            }
            Op::Jump(target) => frame.ip = *target,
            Op::JumpIfFalse(target) => {
                if let Value::Bool(false) = self.pop()? {
                    frame.ip = *target;
                }
            }
            Op::Call(i, count) => self.call(&program.functions[*i], *count, frame)?,
            Op::CallVirtual { slot, args } => {
                let class = self.receiver_class(*args)?;
                let function = program.classes[class].method_table[*slot];
                self.call(&program.functions[function], *args, frame)?;
            }
            Op::CallInterface {
                interface,
                method,
                args,
            } => {
                let class = self.receiver_class(*args)?;
                let function = program.classes[class]
                    .vtables
                    .iter()
                    .find(|vtable| vtable.interface == *interface)
                    .map(|vtable| vtable.functions[*method])
                    .ok_or_else(|| {
                        SomoniError::internal("class does not implement the interface")
                    })?;
                self.call(&program.functions[function], *args, frame)?;
            }
            Op::Return => {
                let result = self.pop()?;
                self.stack.truncate(frame.base);
                *frame = self
                    .callers
                    .pop()
                    .ok_or_else(|| SomoniError::internal("return outside of a function"))?;
                self.stack.push(result);
            }
            Op::MakeList(count) => {
                let start = self.stack.len() - count;
                let items: Vec<Value> = self.stack.drain(start..).collect();
                self.stack.push(Value::List(Rc::new(items)));
            }
            Op::NewMap => self.stack.push(Value::Map(Rc::new(MapValue::default()))),
            Op::MapInsert => {
                let value = self.pop()?;
                let key = map_key(&self.pop()?)?;
                let Some(Value::Map(map)) = self.stack.last_mut() else {
                    return Err(SomoniError::internal("map insert without a map"));
                };
                if map.contains(&key) {
                    return Err(ErrorCode::DuplicateKey {
                        key: show_key(&key),
                    }
                    .into());
                }
                Rc::make_mut(map).insert(key, value);
            }
            Op::NewStruct(i) => {
                let decl = Rc::clone(&program.structs[*i]);
                let fields = vec![Value::Void; decl.fields.len()];
                self.stack
                    .push(Value::Struct(Rc::new(StructValue { decl, fields })));
            }
            Op::NewObject(i) => {
                let layout = Rc::clone(&program.classes[*i].layout);
                self.stack.push(Value::Object(Object::new(layout, *i)));
            }
            Op::InitField(position) => {
                let value = self.pop()?;
                match self.stack.last_mut() {
                    Some(Value::Struct(target)) => Rc::make_mut(target).fields[*position] = value,
                    Some(Value::Object(object)) => object.0.borrow_mut().fields[*position] = value,
                    _ => return Err(SomoniError::internal("field without a struct")),
                }
            }
            Op::InitLocalField(slot, position) => {
                let value = self.pop()?;
                let Value::Object(object) = &self.stack[frame.base + slot] else {
                    return Err(SomoniError::internal("field of a non-object"));
                };
                object.0.borrow_mut().fields[*position] = value;
            }
            Op::Index => {
                let index = self.pop()?;
                let container = self.pop()?;
                self.stack.push(value::index(&container, &index)?);
            }
            Op::Field(position) => {
                let target = self.pop()?;
                self.stack.push(value::field(&target, *position)?);
            }
            Op::AppendLocal(slot) => {
                let rest = self.pop()?;
                let target = &mut self.stack[frame.base + slot];
                if let Value::Str(_) | Value::List(_) = target {
                    value::append(target, rest)?;
                } else {
                    let left = std::mem::replace(target, Value::Void);
                    *target = value::binary(BinaryOp::Add, left, rest)?;
                }
            }
            Op::UpdatePart(i, op) => {
                let path = &frame.chunk.paths[*i];
                let value = self.pop()?;
                let steps = self.take_path_steps(path);
                let target = &mut self.stack[frame.base + path.slot];
                value::change_part(target, steps, false, path.in_object, |place| {
                    let old = std::mem::replace(place, Value::Void);
                    *place = value::binary(*op, old, value)?;
                    Ok(())
                })?;
            }
            Op::Switch(table) => {
                let variant = match self.pop()? {
                    Value::Enum(value) => value.variant,
                    Value::Ok(inner) => {
                        self.stack.push(Rc::unwrap_or_clone(inner));
                        0
                    }
                    Value::Err(inner) => {
                        self.stack.push(Rc::unwrap_or_clone(inner));
                        1
                    }
                    _ => return Err(SomoniError::internal("match on a non-enum")),
                };
                frame.ip = frame.chunk.tables[*table][variant];
            }
            Op::Chars => {
                let Value::Str(text) = self.pop()? else {
                    return Err(SomoniError::internal("characters of a non-string"));
                };
                let chars = text
                    .as_str()
                    .chars()
                    .map(|c| Value::Str(Rc::new(Text::new(c.to_string()))))
                    .collect();
                self.stack.push(Value::List(Rc::new(chars)));
            }
            Op::Next { slot, end, both } => {
                let place = frame.base + slot;
                let Value::Int(i) = self.stack[place + 1] else {
                    return Err(SomoniError::internal("loop index is not an int"));
                };
                let position = i as usize;
                let entry = match &self.stack[place] {
                    Value::List(items) => items
                        .get(position)
                        .map(|item| (Value::Int(i), item.clone(), true)),
                    Value::Map(map) => map
                        .nth(position)
                        .map(|(key, value)| (key.to_value(), value.clone(), false)),
                    _ => return Err(SomoniError::internal("loop over a non-list")),
                };
                let Some((key, item, is_list)) = entry else {
                    frame.ip = *end;
                    return Ok(());
                };
                self.stack[place + 1] = Value::Int(i + 1);
                match (*both, is_list) {
                    (true, _) => {
                        self.stack.push(key);
                        self.stack.push(item);
                    }
                    (false, true) => self.stack.push(item),
                    (false, false) => self.stack.push(key),
                }
            }
            Op::SetPart(i) => {
                let path = &frame.chunk.paths[*i];
                let value = self.pop()?;
                let steps = self.take_path_steps(path);
                let target = &mut self.stack[frame.base + path.slot];
                value::change_part(target, steps, true, path.in_object, |place| {
                    *place = value;
                    Ok(())
                })?;
            }
        }
        Ok(())
    }

    /// Starts a call of `chunk`. Its arguments are already on top of the stack and become its
    /// first variables. The current frame waits in `callers` until the function returns.
    fn call(
        &mut self,
        chunk: &'p Chunk,
        count: usize,
        frame: &mut Frame<'p>,
    ) -> Result<(), SomoniError> {
        if self.callers.len() >= MAX_CALL_DEPTH {
            return Err(ErrorCode::RecursionTooDeep {
                limit: MAX_CALL_DEPTH,
            }
            .into());
        }
        let base = self.stack.len() - count;
        self.stack.resize(base + chunk.locals, Value::Void);
        let callee = Frame { chunk, ip: 0, base };
        self.callers.push(std::mem::replace(frame, callee));
        Ok(())
    }

    /// Returns the number of the class of the object a method is called on: the first of the
    /// `args` arguments on top of the stack.
    fn receiver_class(&self, args: usize) -> Result<usize, SomoniError> {
        match self.stack.get(self.stack.len() - args) {
            Some(Value::Object(object)) => Ok(object.0.borrow().class),
            _ => Err(SomoniError::internal("method call without an object")),
        }
    }

    /// Takes the indices of `path` from the stack and returns its steps with them.
    fn take_path_steps(&mut self, path: &Path) -> Vec<Step> {
        let count = path
            .steps
            .iter()
            .filter(|step| matches!(step, PathStep::Index(_)))
            .count();
        let start = self.stack.len() - count;
        let mut indices = self.stack.drain(start..);
        let mut steps = Vec::with_capacity(path.steps.len());
        for step in &path.steps {
            steps.push(match step {
                PathStep::Index(span) => Step::Index {
                    index: indices.next().unwrap_or(Value::Void),
                    span: *span,
                },
                PathStep::Field(position) => Step::Field(*position),
            });
        }
        steps
    }

    /// Takes the top value from the stack.
    fn pop(&mut self) -> Result<Value, SomoniError> {
        self.stack
            .pop()
            .ok_or_else(|| SomoniError::internal("the stack is empty"))
    }
}
