use std::rc::Rc;

use crate::ast::{BinaryOp, ERR_VARIANT, InstanceTarget, OK_VARIANT, StructDecl};
use crate::builtins::Builtin;
use crate::bytecode::{Chunk, Op, Path, PathRoot, PathStep, Program, ValueTable, index_slot};
use crate::error::{ErrorCode, SomoniError};
use crate::map::MapValue;
use crate::text::Text;
use crate::value::{self, Closure, Object, Step, StructValue, Value, Way, map_key, show_key};

mod higher;
mod output;

use output::Output;

/// More nested calls than this are runtime error R004.
const MAX_CALL_DEPTH: usize = 10_000;

/// Runs a compiled program.
pub fn run(program: &Program) -> Result<(), SomoniError> {
    Vm::default().run(program)
}

/// The request that `somoni serve` answers with a program: its decoded path and the pairs of its query.
#[derive(Debug, Default)]
pub struct Request {
    pub path: String,
    pub query: Vec<(String, String)>,
}

impl Request {
    /// Returns the first value of `name` in the query, like `a` for `?q=a`.
    fn value(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(found, _)| found == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Runs a compiled program for `request` and returns what it writes instead of writing it out, and how it ended.
pub fn run_captured(program: &Program, request: Request) -> (String, Result<(), SomoniError>) {
    let mut vm = Vm {
        request: Some(request),
        ..Vm::default()
    };
    vm.output.start_capture();
    let ended = vm.run(program);
    (vm.output.end_capture(), ended)
}

/// One running function: its chunk, its next instruction `ip` and the stack place `base` of its first variable.
struct Frame<'p> {
    chunk: &'p Chunk,
    ip: usize,
    base: usize,
}

impl Frame<'_> {
    /// Returns the stack place of variable slot `slot`.
    fn place(&self, slot: usize) -> usize {
        self.base + slot
    }
}

/// The values of one round of a `for`-each loop.
struct LoopEntry {
    /// The index of a list element or the key of a map entry.
    key: Value,
    /// The list element or the map value.
    value: Value,
    /// True for a list, whose single loop variable gets the element instead of the index.
    is_list: bool,
}

/// The virtual machine: one stack with the variables and values of all running functions, and the waiting callers.
#[derive(Default)]
struct Vm<'p> {
    stack: Vec<Value>,
    callers: Vec<Frame<'p>>,
    /// The values of the static fields, by their global number.
    globals: Vec<Value>,
    /// Where `print` and `write` go.
    output: Output,
    /// The request of `somoni serve`, which `query` and `request_path` read; None outside of it.
    request: Option<Request>,
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
        self.globals = vec![Value::Void; program.globals.len()];
        self.run_until(&mut frame, program, None)
    }

    /// Runs instructions until the main chunk ends or, with `depth`, until only `depth` callers wait again.
    /// It is the only place that calls `execute`, so the compiler can build `execute` into this loop.
    fn run_until(
        &mut self,
        frame: &mut Frame<'p>,
        program: &'p Program,
        depth: Option<usize>,
    ) -> Result<(), SomoniError> {
        while depth.is_none_or(|depth| self.callers.len() > depth) {
            let Some(op) = frame.chunk.code.get(frame.ip) else {
                return Ok(());
            };
            let (chunk, at) = (frame.chunk, frame.ip);
            frame.ip += 1;
            self.execute(op, frame, program)
                .map_err(|error| error.with_span_if_missing(Some(chunk.spans[at])))?;
        }
        Ok(())
    }

    /// Calls the function value `function` with `args`, runs it until it returns and gives back its result.
    /// Built-in methods like `l.map(f)` use it; `frame` is the running chunk, which goes on afterwards.
    fn call_now(
        &mut self,
        function: &Value,
        args: &[Value],
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<Value, SomoniError> {
        let depth = self.callers.len();
        self.stack.push(function.clone());
        self.stack.extend(args.iter().cloned());
        self.call_value(args.len(), frame, program)?;
        self.run_until(frame, program, Some(depth))?;
        self.pop()
    }

    /// Runs one instruction of `frame`; calls and returns switch `frame` to another chunk.
    fn execute(
        &mut self,
        op: &Op,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<(), SomoniError> {
        match op {
            Op::Push(i) => self.stack.push(copy_value(&frame.chunk.constants[*i])),
            Op::GetLocal(slot) => {
                let value = copy_value(&self.stack[frame.place(*slot)]);
                self.stack.push(value);
            }
            Op::GetGlobal(i) => self.stack.push(self.globals[*i].clone()),
            Op::SetGlobal(i) => {
                let value = self.pop()?;
                self.globals[*i] = value;
            }
            Op::GetConst(i) => self.get_const(*i, program)?,
            Op::SetLocal(slot) => {
                let value = self.pop()?;
                self.stack[frame.place(*slot)] = value;
            }
            Op::Pop => {
                self.pop()?;
            }
            Op::Binary(op) => self.binary(*op)?,
            Op::Unary(op) => {
                let operand = self.pop()?;
                self.stack.push(value::unary(*op, operand)?);
            }
            Op::CallBuiltin(builtin, count) => self.call_builtin(*builtin, *count)?,
            Op::CallHigher(builtin, count) => self.call_higher(*builtin, *count, frame, program)?,
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
                    .interface_function(*interface, *method)
                    .ok_or_else(|| {
                        SomoniError::internal("class does not implement the interface")
                    })?;
                self.call(&program.functions[function], *args, frame)?;
            }
            Op::Return => self.return_to_caller(frame)?,
            Op::Try => self.try_result(frame)?,
            Op::CallValue(count) => self.call_value(*count, frame, program)?,
            Op::MakeClosure { lambda, captured } => self.make_closure(*lambda, *captured),
            Op::GetCaptured(i) => self.get_captured(*i, frame)?,
            Op::MakeTuple(count) => self.make_tuple(*count),
            Op::MakeList(count) => self.make_list(*count),
            Op::NewMap => self.stack.push(Value::Map(Rc::new(MapValue::default()))),
            Op::MapInsert => self.map_insert()?,
            Op::NewStruct(i) => self.stack.push(new_struct(&program.structs[*i])),
            Op::NewObject(i) => {
                let layout = Rc::clone(&program.classes[*i].layout);
                self.stack.push(Value::Object(Object::new(layout, *i)));
            }
            Op::InitField(position) => self.init_field(*position)?,
            Op::InitLocalField(slot, position) => {
                self.init_local_field(frame.place(*slot), *position)?
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
            Op::AppendLocal(slot) => self.append_local(frame.place(*slot))?,
            Op::UpdatePart(i, op) => {
                let op = *op;
                self.change_part(frame, *i, false, |place, value| {
                    let old = std::mem::replace(place, Value::Void);
                    *place = value::binary(op, old, value)?;
                    Ok(())
                })?;
            }
            Op::Switch(table) => {
                let variant = self.pop_variant()?;
                frame.ip = frame.chunk.tables[*table][variant];
            }
            Op::SwitchValue(table) => {
                frame.ip = self.switch_value(&frame.chunk.value_tables[*table])?;
            }
            Op::Chars => self.chars()?,
            Op::InstanceOf(target) => self.instance_of(*target, program)?,
            Op::Next { slot, end, both } => {
                if !self.next_round(frame.place(*slot), *both)? {
                    frame.ip = *end;
                }
            }
            Op::SetPart(i) => self.change_part(frame, *i, true, |place, value| {
                *place = value;
                Ok(())
            })?,
        }
        Ok(())
    }

    /// Pops the right and the left operand and pushes `left op right`; two ints take a short way.
    fn binary(&mut self, op: BinaryOp) -> Result<(), SomoniError> {
        if let [.., Value::Int(a), Value::Int(b)] = self.stack.as_slice() {
            let result = value::int_binary(op, *a, *b)?;
            let left = self.stack.len() - 2;
            self.pop_int();
            self.overwrite_int(left, result);
            return Ok(());
        }
        let right = self.pop()?;
        let left = self.pop()?;
        self.stack.push(value::binary(op, left, right)?);
        Ok(())
    }

    /// Calls a built-in function with the `count` values on top of the stack and pushes its result.
    fn call_builtin(&mut self, builtin: Builtin, count: usize) -> Result<(), SomoniError> {
        let start = self.top_start(count);
        let args = &self.stack[start..];
        let result = if let Some(value) = self.request_value(builtin, args) {
            value
        } else if let Some(text) = builtin.output_text(args) {
            self.output.write(&text)?;
            Value::Void
        } else {
            builtin.call(args)?
        };
        self.stack.truncate(start);
        self.stack.push(result);
        Ok(())
    }

    /// Returns the value of `query(name)` or `request_path()` for the request being answered, None for other functions;
    /// without a request, `query` gives null and `request_path` an empty string.
    fn request_value(&self, builtin: Builtin, args: &[Value]) -> Option<Value> {
        let text = |text: &str| Value::Str(Rc::new(Text::new(text.to_string())));
        match (builtin, args) {
            (Builtin::Query, [Value::Str(name)]) => {
                let found = self
                    .request
                    .as_ref()
                    .and_then(|request| request.value(name.as_str()));
                Some(found.map_or(Value::Null, text))
            }
            (Builtin::RequestPath, []) => {
                let path = self
                    .request
                    .as_ref()
                    .map_or("", |request| request.path.as_str());
                Some(text(path))
            }
            _ => None,
        }
    }

    /// Starts a call of `chunk` whose `count` arguments are on top of the stack, while `frame` waits in `callers`.
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
        let base = self.top_start(count);
        self.stack.resize(base + chunk.locals, Value::Void);
        let callee = Frame { chunk, ip: 0, base };
        self.callers.push(std::mem::replace(frame, callee));
        Ok(())
    }

    /// Ends the running function and goes back to its caller with the result.
    fn return_to_caller(&mut self, frame: &mut Frame<'p>) -> Result<(), SomoniError> {
        let result = self.pop()?;
        self.stack.truncate(frame.base);
        *frame = self
            .callers
            .pop()
            .ok_or_else(|| SomoniError::internal("return outside of a function"))?;
        self.stack.push(result);
        Ok(())
    }

    /// Pops `count` values and pushes a tuple of them.
    #[inline(never)]
    fn make_tuple(&mut self, count: usize) {
        let items = self.stack.split_off(self.top_start(count));
        self.stack.push(Value::Tuple(Rc::new(items)));
    }

    /// Pops `count` copies and pushes the value of lambda number `lambda` with them.
    #[inline(never)]
    fn make_closure(&mut self, lambda: usize, count: usize) {
        let captured = self.stack.split_off(self.top_start(count));
        let closure = Closure { lambda, captured };
        self.stack.push(Value::Closure(Rc::new(closure)));
    }

    /// Pushes copy number `i` of the running lambda, whose value is in slot 0 of its frame.
    #[inline(never)]
    fn get_captured(&mut self, i: usize, frame: &Frame<'p>) -> Result<(), SomoniError> {
        let Some(Value::Closure(closure)) = self.stack.get(frame.base) else {
            return Err(SomoniError::internal("copy outside of a lambda"));
        };
        let value = closure.captured[i].clone();
        self.stack.push(value);
        Ok(())
    }

    /// Calls the function value below the `count` arguments; a lambda keeps its value there as slot 0 for its copies.
    #[inline(never)]
    fn call_value(
        &mut self,
        count: usize,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<(), SomoniError> {
        let place = self.top_start(count) - 1;
        match &self.stack[place] {
            Value::Function(function) => {
                let function = *function;
                self.stack.remove(place);
                self.call(&program.functions[function], count, frame)
            }
            Value::Closure(closure) => {
                let chunk = &program.lambdas[closure.lambda];
                self.call(chunk, count + 1, frame)
            }
            _ => Err(SomoniError::internal("call of a value that is no function")),
        }
    }

    /// Pops a result: pushes the value of an `ok`, or returns the whole `err` to the caller.
    #[inline(never)]
    fn try_result(&mut self, frame: &mut Frame<'p>) -> Result<(), SomoniError> {
        match self.pop()? {
            Value::Ok(value) => self.stack.push(Rc::unwrap_or_clone(value)),
            err @ Value::Err(_) => {
                self.stack.push(err);
                self.return_to_caller(frame)?;
            }
            _ => return Err(SomoniError::internal("'?' on a value that is no result")),
        }
        Ok(())
    }

    /// Returns the class number of the object a method is called on, the lowest of its `args` values.
    fn receiver_class(&self, args: usize) -> Result<usize, SomoniError> {
        match self.stack.get(self.top_start(args)) {
            Some(Value::Object(object)) => Ok(object.0.borrow().class),
            _ => Err(SomoniError::internal("method call without an object")),
        }
    }

    /// Pops `count` values and pushes a list of them.
    #[inline(never)]
    fn make_list(&mut self, count: usize) {
        let start = self.top_start(count);
        let items: Vec<Value> = self.stack.drain(start..).collect();
        self.stack.push(Value::List(Rc::new(items)));
    }

    /// Pops a value and a key and adds them to the map below, or returns R007 if the key is already there.
    #[inline(never)]
    fn map_insert(&mut self) -> Result<(), SomoniError> {
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
        Ok(())
    }

    /// Pops a value and sets field `position` of the struct or object below.
    #[inline(never)]
    fn init_field(&mut self, position: usize) -> Result<(), SomoniError> {
        let value = self.pop()?;
        match self.stack.last_mut() {
            Some(Value::Struct(target)) => Rc::make_mut(target).fields[position] = value,
            Some(Value::Object(object)) => object.0.borrow_mut().fields[position] = value,
            _ => return Err(SomoniError::internal("field without a struct")),
        }
        Ok(())
    }

    /// Pops a value and sets field `position` of the object at stack place `place`.
    #[inline(never)]
    fn init_local_field(&mut self, place: usize, position: usize) -> Result<(), SomoniError> {
        let value = self.pop()?;
        let Value::Object(object) = &self.stack[place] else {
            return Err(SomoniError::internal("field of a non-object"));
        };
        object.0.borrow_mut().fields[position] = value;
        Ok(())
    }

    /// Removes the top value, which the caller has checked to be an int.
    fn pop_int(&mut self) {
        // An int owns nothing on the heap, so forgetting it safely skips the call that would drop it.
        std::mem::forget(self.stack.pop());
    }

    /// Writes `value` at stack place `place`, where the caller has checked an int to be.
    fn overwrite_int(&mut self, place: usize, value: Value) {
        std::mem::forget(std::mem::replace(&mut self.stack[place], value));
    }

    /// Pops a value and appends it to the string or list at stack place `place`, or adds it to a number there.
    fn append_local(&mut self, place: usize) -> Result<(), SomoniError> {
        if let (Some(Value::Int(rest)), Some(Value::Int(sum))) =
            (self.stack.last(), self.stack.get(place))
        {
            let result = value::int_binary(BinaryOp::Add, *sum, *rest)?;
            self.pop_int();
            self.overwrite_int(place, result);
            return Ok(());
        }
        let rest = self.pop()?;
        let target = &mut self.stack[place];
        if let Value::Str(_) | Value::List(_) = target {
            return value::append(target, rest);
        }
        let left = std::mem::replace(target, Value::Void);
        *target = value::binary(BinaryOp::Add, left, rest)?;
        Ok(())
    }

    /// Pops a value and the indices of path number `i`, and lets `change` write the value into that part.
    #[inline(never)]
    fn change_part(
        &mut self,
        frame: &Frame<'p>,
        i: usize,
        may_add_key: bool,
        change: impl FnOnce(&mut Value, Value) -> Result<(), SomoniError>,
    ) -> Result<(), SomoniError> {
        let path = &frame.chunk.paths[i];
        let value = self.pop()?;
        let way = self.take_way(path);
        let target = match path.root {
            PathRoot::Local(slot) => &mut self.stack[frame.place(slot)],
            PathRoot::Global(global) => &mut self.globals[global],
        };
        value::change_part(target, way, may_add_key, |place| change(place, value))
    }

    /// Takes the indices of `path` from the stack and returns the way along its steps with them.
    fn take_way(&mut self, path: &Path) -> Way {
        let start = self.top_start(path.index_count());
        let mut indices = self.stack.drain(start..);
        let steps = path
            .steps
            .iter()
            .map(|step| match step {
                PathStep::Index(span) => Step::Index {
                    index: indices.next().unwrap_or(Value::Void),
                    span: *span,
                },
                PathStep::Field(position) => Step::Field(*position),
            })
            .collect();
        Way {
            steps,
            through_object: path.in_object,
        }
    }

    /// Pops an enum or result and returns its variant number, pushing the value inside a result.
    #[inline(never)]
    fn pop_variant(&mut self) -> Result<usize, SomoniError> {
        match self.pop()? {
            Value::Enum(value) => Ok(value.variant),
            Value::Ok(inner) => {
                self.stack.push(Rc::unwrap_or_clone(inner));
                Ok(OK_VARIANT)
            }
            Value::Err(inner) => {
                self.stack.push(Rc::unwrap_or_clone(inner));
                Ok(ERR_VARIANT)
            }
            _ => Err(SomoniError::internal("match on a non-enum")),
        }
    }

    /// Pops the value of a `match` over ints or strings and returns where its branch starts.
    #[inline(never)]
    fn switch_value(&mut self, table: &ValueTable) -> Result<usize, SomoniError> {
        let value = self.pop()?;
        let found = table.cases.iter().find(|(case, _)| *case == value);
        Ok(found.map_or(table.rest, |(_, start)| *start))
    }

    /// Pushes constant number `i`, or returns R008 if its line has not run yet.
    #[inline(never)]
    fn get_const(&mut self, i: usize, program: &Program) -> Result<(), SomoniError> {
        if matches!(self.globals[i], Value::Void) {
            return Err(const_before_value(i, program));
        }
        self.stack.push(self.globals[i].clone());
        Ok(())
    }

    /// Pops a value and pushes whether it is an object of the class or interface `target`.
    #[inline(never)]
    fn instance_of(
        &mut self,
        target: InstanceTarget,
        program: &Program,
    ) -> Result<(), SomoniError> {
        let is_instance = match self.pop()? {
            Value::Object(object) => program.is_instance(object.0.borrow().class, target),
            _ => false,
        };
        self.stack.push(Value::Bool(is_instance));
        Ok(())
    }

    /// Pops a string and pushes the list of its characters.
    #[inline(never)]
    fn chars(&mut self) -> Result<(), SomoniError> {
        let Value::Str(text) = self.pop()? else {
            return Err(SomoniError::internal("characters of a non-string"));
        };
        let chars = text
            .as_str()
            .chars()
            .map(|c| Value::Str(Rc::new(Text::new(c.to_string()))))
            .collect();
        self.stack.push(Value::List(Rc::new(chars)));
        Ok(())
    }

    /// Pushes the next round of a `for`-each loop over the sequence at `place` and returns false at its end.
    #[inline(never)]
    fn next_round(&mut self, place: usize, both: bool) -> Result<bool, SomoniError> {
        let Value::Int(index) = self.stack[index_slot(place)] else {
            return Err(SomoniError::internal("loop index is not an int"));
        };
        let Some(entry) = loop_entry(&self.stack[place], index)? else {
            return Ok(false);
        };
        self.stack[index_slot(place)] = Value::Int(index + 1);
        self.push_entry(entry, both);
        Ok(true)
    }

    /// Pushes what the loop variables get: with `both` the key and the value, else one of them.
    fn push_entry(&mut self, entry: LoopEntry, both: bool) {
        match (both, entry.is_list) {
            (true, _) => {
                self.stack.push(entry.key);
                self.stack.push(entry.value);
            }
            (false, true) => self.stack.push(entry.value),
            (false, false) => self.stack.push(entry.key),
        }
    }

    /// Returns the stack place of the lowest of the `count` values on top.
    fn top_start(&self, count: usize) -> usize {
        self.stack.len() - count
    }

    /// Takes the top value from the stack.
    fn pop(&mut self) -> Result<Value, SomoniError> {
        self.stack
            .pop()
            .ok_or_else(|| SomoniError::internal("the stack is empty"))
    }
}

/// Returns the error R008 for constant number `i`, which is still `void` because its line has not run yet.
#[inline(never)]
fn const_before_value(i: usize, program: &Program) -> SomoniError {
    let name = program.globals[i].clone();
    ErrorCode::ConstBeforeValue { name }.into()
}

/// Returns a new value of a struct whose fields are not set yet.
fn new_struct(decl: &Rc<StructDecl>) -> Value {
    let fields = vec![Value::Void; decl.fields.len()];
    Value::Struct(Rc::new(StructValue {
        decl: Rc::clone(decl),
        fields,
    }))
}

/// Returns the entry at `index` of the list or map a `for`-each loop runs over, or None past its end.
fn loop_entry(sequence: &Value, index: i64) -> Result<Option<LoopEntry>, SomoniError> {
    let position = index as usize;
    match sequence {
        Value::List(items) => Ok(items.get(position).map(|item| LoopEntry {
            key: Value::Int(index),
            value: item.clone(),
            is_list: true,
        })),
        Value::Map(map) => Ok(map.nth(position).map(|(key, value)| LoopEntry {
            key: key.to_value(),
            value: value.clone(),
            is_list: false,
        })),
        _ => Err(SomoniError::internal("loop over a non-list")),
    }
}

/// Returns a copy of a value; an int, the most common value, is copied directly instead of through `clone`.
#[inline]
fn copy_value(value: &Value) -> Value {
    match value {
        Value::Int(n) => Value::Int(*n),
        other => other.clone(),
    }
}
