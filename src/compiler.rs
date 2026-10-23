use std::rc::Rc;

use crate::ast::{
    self, Assign, BinaryOp, CallTarget, ClassDecl, ClassLayout, EnumDecl, Expr, ExprKind,
    FieldInit, ForEach, Function, IterKind, Match, MethodTarget, Pattern, Stmt,
};
use crate::bytecode::{Chunk, ClassCode, Op, Path, PathStep, Program};
use crate::error::SomoniError;
use crate::loader::SourceFile;
use crate::span::Span;
use crate::value::{EnumValue, Value};

/// The slot of `this` in a method or `init`.
const THIS_SLOT: usize = 0;

/// Turns a checked program into bytecode, file by file in `order` like the type checker.
pub fn compile(files: &[SourceFile], order: &[usize]) -> Result<Program, SomoniError> {
    let mut program = Program {
        main: compile_main(files)?,
        ..Program::default()
    };
    for &number in order {
        add_file(&mut program, &files[number].stmts)?;
    }
    Ok(program)
}

/// Compiles the top-level code of the main file.
fn compile_main(files: &[SourceFile]) -> Result<Chunk, SomoniError> {
    let main = filled(files.first(), "main file")?;
    let mut compiler = Compiler::new("program".to_string(), 0);
    compiler.compile_stmts(&main.stmts)?;
    Ok(compiler.chunk)
}

/// Adds the interfaces, functions, classes and structs of one file to the program.
fn add_file(program: &mut Program, stmts: &[Stmt]) -> Result<(), SomoniError> {
    let interfaces = ast::interfaces(stmts).map(|decl| Rc::clone(&decl.layout));
    program.interfaces.extend(interfaces);
    for function in ast::functions(stmts) {
        let name = format!("fn {}", function.name);
        let chunk = compile_function(function, name, function.params.len())?;
        program.functions.push(chunk);
    }
    for class in ast::classes(stmts) {
        add_class(program, class)?;
    }
    program.structs.extend(ast::structs(stmts).cloned());
    Ok(())
}

/// Adds the `init`, the methods and the class code of one class to the program.
fn add_class(program: &mut Program, class: &ClassDecl) -> Result<(), SomoniError> {
    program.functions.push(compile_init(class)?);
    for method in class.methods.iter().map(|m| &m.function) {
        let name = format!("{}.{}", class.layout.name, method.name);
        let chunk = compile_function(method, name, with_this(method.params.len()))?;
        program.functions.push(chunk);
    }
    program.classes.push(class_code(class));
    Ok(())
}

/// Returns the layout and tables of a class as the VM needs them.
fn class_code(class: &ClassDecl) -> ClassCode {
    let layout = ClassLayout {
        name: class.layout.name.clone(),
        fields: class.plan.fields.clone(),
    };
    ClassCode {
        layout: Rc::new(layout),
        method_table: class.plan.methods.clone(),
        vtables: class.plan.vtables.clone(),
    }
}

/// Compiles `init Name`, which sets up a new object and returns `this`.
fn compile_init(class: &ClassDecl) -> Result<Chunk, SomoniError> {
    let name = format!("init {}", class.layout.name);
    let mut compiler = Compiler {
        returns_this: true,
        ..Compiler::new(name, with_this(class.plan.params))
    };
    let body = compiler.compile_parent_init(class)?;
    compiler.compile_field_starts(class)?;
    compiler.compile_stmts(body)?;
    compiler
        .chunk
        .push(Op::GetLocal(THIS_SLOT), class.name_span);
    compiler.chunk.push(Op::Return, class.name_span);
    Ok(compiler.chunk)
}

/// Compiles a function or method that returns `void` when it reaches its end.
fn compile_function(
    function: &Function,
    name: String,
    params: usize,
) -> Result<Chunk, SomoniError> {
    let mut compiler = Compiler::new(name, params);
    compiler.compile_stmts(&function.body)?;
    compiler.push_constant(Value::Void, function.name_span);
    compiler.chunk.push(Op::Return, function.name_span);
    Ok(compiler.chunk)
}

/// Returns the number of values with `this` in front of `count` others.
fn with_this(count: usize) -> usize {
    count + 1
}

/// Returns the slot of the hidden index of a `for`-each loop, which the VM keeps after the sequence.
fn index_slot(seq: usize) -> usize {
    seq + 1
}

/// Returns a value an earlier step filled in, or an internal error naming what is missing.
fn filled<T>(value: Option<T>, what: &str) -> Result<T, SomoniError> {
    value.ok_or_else(|| SomoniError::internal(format!("missing {what}")))
}

/// Returns the instruction for a method call with `args` values on the stack.
fn method_op(call: MethodTarget, args: usize) -> Op {
    match call {
        MethodTarget::Virtual { slot } => Op::CallVirtual { slot, args },
        MethodTarget::Interface { interface, method } => Op::CallInterface {
            interface,
            method,
            args,
        },
    }
}

/// Writes the instructions of one chunk.
#[derive(Default)]
struct Compiler {
    chunk: Chunk,
    loops: Vec<LoopJumps>,
    /// True in `init Name`, where `return;` gives back `this`.
    returns_this: bool,
}

/// The jumps of one loop that are patched when the loop ends.
#[derive(Default)]
struct LoopJumps {
    start: usize,
    exit: usize,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl Compiler {
    /// Starts an empty chunk whose first `params` slots are its parameters.
    fn new(name: String, params: usize) -> Compiler {
        let mut compiler = Compiler::default();
        compiler.chunk.name = name;
        compiler.chunk.locals = params;
        compiler
    }

    /// Runs the `init` of the parent class and returns the rest of the constructor.
    fn compile_parent_init<'a>(&mut self, class: &'a ClassDecl) -> Result<&'a [Stmt], SomoniError> {
        let body: &[Stmt] = class.constructor.as_ref().map_or(&[], |c| &c.body);
        let Some(parent_init) = class.plan.parent_init else {
            return Ok(body);
        };
        if let Some((super_init, rest)) = body.split_first() {
            self.compile_stmt(super_init)?;
            return Ok(rest);
        }
        self.call_inherited_init(parent_init, class.plan.params, class.name_span);
        Ok(body)
    }

    /// Calls the inherited `init` with `this` and the own arguments.
    fn call_inherited_init(&mut self, parent_init: usize, params: usize, span: Span) {
        for slot in 0..with_this(params) {
            self.chunk.push(Op::GetLocal(slot), span);
        }
        self.chunk
            .push(Op::Call(parent_init, with_this(params)), span);
        self.chunk.push(Op::Pop, span);
    }

    /// Sets the own fields that have a start value.
    fn compile_field_starts(&mut self, class: &ClassDecl) -> Result<(), SomoniError> {
        for (position, field) in class.fields.iter().enumerate() {
            if let Some(start) = &field.start {
                self.compile_expr(start)?;
                let field = class.plan.own_field(position);
                self.chunk
                    .push(Op::InitLocalField(THIS_SLOT, field), start.span);
            }
        }
        Ok(())
    }

    /// Writes statements one after another.
    fn compile_stmts(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    /// Writes a statement, leaving the stack as before.
    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), SomoniError> {
        match stmt {
            Stmt::VarDecl {
                value, slot, span, ..
            } => {
                self.compile_expr(value)?;
                let slot = self.use_slot(filled(*slot, "variable slot")?);
                self.chunk.push(Op::SetLocal(slot), *span);
            }
            Stmt::Assign(assign) => self.compile_assign(assign)?,
            Stmt::Expr { expr, span } => {
                self.compile_expr(expr)?;
                self.chunk.push(Op::Pop, *span);
            }
            Stmt::Block(stmts) => self.compile_stmts(stmts)?,
            Stmt::FnDecl(_)
            | Stmt::StructDecl(_)
            | Stmt::EnumDecl(_)
            | Stmt::ClassDecl(_)
            | Stmt::InterfaceDecl(_)
            | Stmt::Import(_) => {}
            Stmt::Match(m) => self.compile_match(m)?,
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => self.compile_if(condition, then_branch, else_branch.as_deref(), *span)?,
            Stmt::While {
                condition, body, ..
            } => self.compile_loop(condition, body, None)?,
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => {
                self.compile_stmt(init)?;
                self.compile_loop(condition, body, Some(step))?;
            }
            Stmt::ForEach(each) => self.compile_for_each(each)?,
            Stmt::Break { span } => {
                let jump = self.jump_forward(*span);
                self.current_loop()?.breaks.push(jump);
            }
            Stmt::Continue { span } => {
                let jump = self.jump_forward(*span);
                self.current_loop()?.continues.push(jump);
            }
            Stmt::Return { value, span } => self.compile_return(value.as_ref(), *span)?,
        }
        Ok(())
    }

    /// Writes an assignment to a variable or to a part of one.
    fn compile_assign(&mut self, assign: &Assign) -> Result<(), SomoniError> {
        let op = assign.op.map(|(op, _)| op);
        match &assign.target.kind {
            ExprKind::Ident { slot, .. } => {
                let slot = filled(*slot, "variable slot")?;
                self.compile_set_local(slot, op, &assign.value, assign.span)
            }
            _ => self.compile_set_part(assign, op),
        }
    }

    /// Writes an assignment to a local variable, appending in place where possible.
    fn compile_set_local(
        &mut self,
        slot: usize,
        op: Option<BinaryOp>,
        value: &Expr,
        span: Span,
    ) -> Result<(), SomoniError> {
        let slot = self.use_slot(slot);
        match (op, value.appended_to(slot)) {
            (Some(BinaryOp::Add), _) => {
                self.compile_expr(value)?;
                self.chunk.push(Op::AppendLocal(slot), span);
            }
            (Some(op), _) => {
                self.chunk.push(Op::GetLocal(slot), span);
                self.compile_expr(value)?;
                self.chunk.push(Op::Binary(op), span);
                self.chunk.push(Op::SetLocal(slot), span);
            }
            (None, Some(rest)) => {
                self.compile_expr(rest)?;
                self.chunk.push(Op::AppendLocal(slot), value.span);
            }
            (None, None) => {
                self.compile_expr(value)?;
                self.chunk.push(Op::SetLocal(slot), span);
            }
        }
        Ok(())
    }

    /// Writes an assignment to a list element or field, like `l[i].x = v`.
    fn compile_set_part(
        &mut self,
        assign: &Assign,
        op: Option<BinaryOp>,
    ) -> Result<(), SomoniError> {
        let mut steps = Vec::new();
        let slot = self.compile_path(&assign.target, &mut steps)?;
        self.compile_expr(&assign.value)?;
        let path = self.chunk.add_path(Path {
            slot,
            steps,
            in_object: assign.in_object,
        });
        let change = match op {
            None => Op::SetPart(path),
            Some(op) => Op::UpdatePart(path, op),
        };
        self.chunk.push(change, assign.span);
        Ok(())
    }

    /// Writes the indices of an assignment target from left to right and returns its variable slot.
    fn compile_path(
        &mut self,
        target: &Expr,
        steps: &mut Vec<PathStep>,
    ) -> Result<usize, SomoniError> {
        match &target.kind {
            ExprKind::Ident { slot, .. } => Ok(self.use_slot(filled(*slot, "variable slot")?)),
            ExprKind::Index {
                target: inner,
                index,
            } => {
                let slot = self.compile_path(inner, steps)?;
                self.compile_expr(index)?;
                steps.push(PathStep::Index(target.span));
                Ok(slot)
            }
            ExprKind::Field {
                target: inner,
                index,
                ..
            } => {
                let slot = self.compile_path(inner, steps)?;
                steps.push(PathStep::Field(filled(*index, "field position")?));
                Ok(slot)
            }
            _ => Err(SomoniError::internal("assignment to a non-variable")),
        }
    }

    /// Writes a `match` as a jump table over the variants.
    fn compile_match(&mut self, m: &Match) -> Result<(), SomoniError> {
        self.compile_expr(&m.subject)?;
        let table = self.chunk.add_table();
        self.chunk.push(Op::Switch(table), m.span);
        let starts = self.compile_arms(m)?;
        self.chunk.tables[table] = m.arm_of_variant.iter().map(|&arm| starts[arm]).collect();
        Ok(())
    }

    /// Writes the branches of a `match` and returns where each one starts.
    fn compile_arms(&mut self, m: &Match) -> Result<Vec<usize>, SomoniError> {
        let mut starts = Vec::new();
        let mut to_end = Vec::new();
        for arm in &m.arms {
            starts.push(self.chunk.code.len());
            self.compile_arm_value(&arm.pattern, m.unwraps, m.span)?;
            self.compile_stmt(&arm.body)?;
            to_end.push(self.jump_forward(m.span));
        }
        self.patch_jumps(to_end);
        Ok(starts)
    }

    /// Stores or drops the value inside a result that a branch gets.
    fn compile_arm_value(
        &mut self,
        pattern: &Pattern,
        unwraps: bool,
        span: Span,
    ) -> Result<(), SomoniError> {
        match pattern {
            Pattern::Result { slot, .. } => {
                let slot = self.use_slot(filled(*slot, "pattern slot")?);
                self.chunk.push(Op::SetLocal(slot), span);
            }
            Pattern::Rest(_) if unwraps => {
                self.chunk.push(Op::Pop, span);
            }
            Pattern::Variant { .. } | Pattern::Rest(_) => {}
        }
        Ok(())
    }

    /// Writes `if` with an optional `else`.
    fn compile_if(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
        span: Span,
    ) -> Result<(), SomoniError> {
        self.compile_expr(condition)?;
        let to_else = self.jump_forward_if_false(condition.span);
        self.compile_stmt(then_branch)?;
        let Some(else_branch) = else_branch else {
            self.patch_jump(to_else);
            return Ok(());
        };
        let to_end = self.jump_forward(span);
        self.patch_jump(to_else);
        self.compile_stmt(else_branch)?;
        self.patch_jump(to_end);
        Ok(())
    }

    /// Writes a `while` loop, or a `for` loop after its start, where `continue` goes to the step.
    fn compile_loop(
        &mut self,
        condition: &Expr,
        body: &Stmt,
        step: Option<&Stmt>,
    ) -> Result<(), SomoniError> {
        let start = self.chunk.code.len();
        self.compile_expr(condition)?;
        let exit = self.jump_forward_if_false(condition.span);
        let jumps = self.compile_loop_body(body, start, exit)?;
        let next_round = self.chunk.code.len();
        if let Some(step) = step {
            self.compile_stmt(step)?;
        }
        self.finish_loop(jumps, next_round, condition.span);
        Ok(())
    }

    /// Writes a `for`-each loop over a list, map or string.
    fn compile_for_each(&mut self, each: &ForEach) -> Result<(), SomoniError> {
        let seq = self.use_slot(filled(each.seq_slot, "loop slot")?);
        self.compile_sequence(each, seq)?;
        let start = self.chunk.code.len();
        let next = Op::Next {
            slot: seq,
            end: 0,
            both: each.binds_both(),
        };
        let exit = self.chunk.push(next, each.span);
        self.set_loop_vars(&each.slots, each.span);
        let jumps = self.compile_loop_body(&each.body, start, exit)?;
        self.finish_loop(jumps, start, each.span);
        Ok(())
    }

    /// Stores what a `for`-each loop runs over and sets its hidden index to 0.
    fn compile_sequence(&mut self, each: &ForEach, seq: usize) -> Result<(), SomoniError> {
        let index = self.use_slot(index_slot(seq));
        self.compile_expr(&each.iterable)?;
        if let Some(IterKind::Str) = each.over {
            self.chunk.push(Op::Chars, each.iterable.span);
        }
        self.chunk.push(Op::SetLocal(seq), each.span);
        self.push_constant(Value::Int(0), each.span);
        self.chunk.push(Op::SetLocal(index), each.span);
        Ok(())
    }

    /// Stores the values of one round in the loop variables, the last one first.
    fn set_loop_vars(&mut self, slots: &[usize], span: Span) {
        for &slot in slots.iter().rev() {
            let slot = self.use_slot(slot);
            self.chunk.push(Op::SetLocal(slot), span);
        }
    }

    /// Writes a loop body and returns its jumps for `finish_loop`.
    fn compile_loop_body(
        &mut self,
        body: &Stmt,
        start: usize,
        exit: usize,
    ) -> Result<LoopJumps, SomoniError> {
        self.loops.push(LoopJumps {
            start,
            exit,
            ..LoopJumps::default()
        });
        self.compile_stmt(body)?;
        filled(self.loops.pop(), "loop")
    }

    /// Jumps back to the start of a loop and lets `continue`, `break` and the exit find their targets.
    fn finish_loop(&mut self, jumps: LoopJumps, continue_to: usize, span: Span) {
        self.chunk.push(Op::Jump(jumps.start), span);
        self.patch_jumps_to(jumps.continues, continue_to);
        self.patch_jump(jumps.exit);
        self.patch_jumps(jumps.breaks);
    }

    /// Writes `return`, which gives back `this` without a value in `init`.
    fn compile_return(&mut self, value: Option<&Expr>, span: Span) -> Result<(), SomoniError> {
        match (value, self.returns_this) {
            (Some(value), _) => self.compile_expr(value)?,
            (None, true) => {
                self.chunk.push(Op::GetLocal(THIS_SLOT), span);
            }
            (None, false) => self.push_constant(Value::Void, span),
        }
        self.chunk.push(Op::Return, span);
        Ok(())
    }

    /// Returns the jumps of the innermost loop.
    fn current_loop(&mut self) -> Result<&mut LoopJumps, SomoniError> {
        filled(self.loops.last_mut(), "loop of break or continue")
    }

    /// Writes a jump whose target is patched later and returns its position.
    fn jump_forward(&mut self, span: Span) -> usize {
        self.chunk.push(Op::Jump(0), span)
    }

    /// Writes a jump on `false` whose target is patched later and returns its position.
    fn jump_forward_if_false(&mut self, span: Span) -> usize {
        self.chunk.push(Op::JumpIfFalse(0), span)
    }

    /// Lets the jump at `at` go to the next instruction that will be written.
    fn patch_jump(&mut self, at: usize) {
        self.patch_jump_to(at, self.chunk.code.len());
    }

    /// Lets all these jumps go to the next instruction that will be written.
    fn patch_jumps(&mut self, jumps: Vec<usize>) {
        self.patch_jumps_to(jumps, self.chunk.code.len());
    }

    /// Lets all these jumps go to `target`.
    fn patch_jumps_to(&mut self, jumps: Vec<usize>, target: usize) {
        for at in jumps {
            self.patch_jump_to(at, target);
        }
    }

    /// Lets the jump at `at` go to `target`.
    fn patch_jump_to(&mut self, at: usize, target: usize) {
        if let Some(Op::Jump(to) | Op::JumpIfFalse(to) | Op::Next { end: to, .. }) =
            self.chunk.code.get_mut(at)
        {
            *to = target;
        }
    }

    /// Writes an expression, leaving its value on top of the stack.
    fn compile_expr(&mut self, expr: &Expr) -> Result<(), SomoniError> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Int(n) => self.push_constant(Value::Int(*n), span),
            ExprKind::Float(f) => self.push_constant(Value::Float(*f), span),
            ExprKind::Bool(b) => self.push_constant(Value::Bool(*b), span),
            ExprKind::Str(s) => self.push_constant(Value::Str(Rc::clone(s)), span),
            ExprKind::Null => self.push_constant(Value::Null, span),
            ExprKind::EnumValue { decl, variant } => self.push_enum(decl, *variant, span),
            ExprKind::Ident { slot, .. } => {
                let slot = self.use_slot(filled(*slot, "variable slot")?);
                self.chunk.push(Op::GetLocal(slot), span);
            }
            ExprKind::Binary {
                left,
                op: BinaryOp::And,
                right,
                ..
            } => self.compile_and(left, right, span)?,
            ExprKind::Binary {
                left,
                op: BinaryOp::Or,
                right,
                ..
            } => self.compile_or(left, right, span)?,
            ExprKind::Binary {
                left, op, right, ..
            } => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.chunk.push(Op::Binary(*op), span);
            }
            ExprKind::Unary { op, operand } => {
                self.compile_expr(operand)?;
                self.chunk.push(Op::Unary(*op), span);
            }
            ExprKind::Call { args, target, .. } => self.compile_call(args, *target, span)?,
            ExprKind::MethodCall {
                target, args, call, ..
            } => self.compile_method_call(target, args, *call, span)?,
            ExprKind::SuperInit { args, target } | ExprKind::SuperCall { args, target, .. } => {
                self.chunk.push(Op::GetLocal(THIS_SLOT), span);
                self.call_with_this(*target, args, span)?;
            }
            ExprKind::New {
                args,
                class,
                target,
                ..
            } => {
                let class = filled(*class, "class of new")?;
                self.chunk.push(Op::NewObject(class), span);
                self.call_with_this(*target, args, span)?;
            }
            ExprKind::List(elements) => {
                self.compile_args(elements)?;
                self.chunk.push(Op::MakeList(elements.len()), span);
            }
            ExprKind::Map(entries) => self.compile_map(entries, span)?,
            ExprKind::Index { target, index } => {
                self.compile_expr(target)?;
                self.compile_expr(index)?;
                self.chunk.push(Op::Index, span);
            }
            ExprKind::StructLiteral { fields, index, .. } => {
                self.compile_struct_literal(fields, *index, span)?
            }
            ExprKind::Field { target, index, .. } => {
                self.compile_expr(target)?;
                let position = filled(*index, "field position")?;
                self.chunk.push(Op::Field(position), span);
            }
        }
        Ok(())
    }

    /// Writes `left && right`, which skips `right` if `left` is false.
    fn compile_and(&mut self, left: &Expr, right: &Expr, span: Span) -> Result<(), SomoniError> {
        self.compile_expr(left)?;
        let to_false = self.jump_forward_if_false(span);
        self.compile_expr(right)?;
        let to_end = self.jump_forward(span);
        self.patch_jump(to_false);
        self.push_constant(Value::Bool(false), span);
        self.patch_jump(to_end);
        Ok(())
    }

    /// Writes `left || right`, which skips `right` if `left` is true.
    fn compile_or(&mut self, left: &Expr, right: &Expr, span: Span) -> Result<(), SomoniError> {
        self.compile_expr(left)?;
        let to_right = self.jump_forward_if_false(span);
        self.push_constant(Value::Bool(true), span);
        let to_end = self.jump_forward(span);
        self.patch_jump(to_right);
        self.compile_expr(right)?;
        self.patch_jump(to_end);
        Ok(())
    }

    /// Writes a call of a built-in or an own function.
    fn compile_call(
        &mut self,
        args: &[Expr],
        target: Option<CallTarget>,
        span: Span,
    ) -> Result<(), SomoniError> {
        self.compile_args(args)?;
        let op = match filled(target, "call target")? {
            CallTarget::Builtin(builtin) => Op::CallBuiltin(builtin, args.len()),
            CallTarget::Function(i) => Op::Call(i, args.len()),
        };
        self.chunk.push(op, span);
        Ok(())
    }

    /// Writes `target.name(args)`.
    fn compile_method_call(
        &mut self,
        target: &Expr,
        args: &[Expr],
        call: Option<MethodTarget>,
        span: Span,
    ) -> Result<(), SomoniError> {
        self.compile_expr(target)?;
        self.compile_args(args)?;
        let call = filled(call, "method target")?;
        self.chunk
            .push(method_op(call, with_this(args.len())), span);
        Ok(())
    }

    /// Writes the arguments and a call of `target` with the object below them as `this`.
    fn call_with_this(
        &mut self,
        target: Option<usize>,
        args: &[Expr],
        span: Span,
    ) -> Result<(), SomoniError> {
        self.compile_args(args)?;
        let target = filled(target, "function of super or new")?;
        self.chunk
            .push(Op::Call(target, with_this(args.len())), span);
        Ok(())
    }

    /// Writes a map literal, one entry after another.
    fn compile_map(&mut self, entries: &[(Expr, Expr)], span: Span) -> Result<(), SomoniError> {
        self.chunk.push(Op::NewMap, span);
        for (key, value) in entries {
            self.compile_expr(key)?;
            self.compile_expr(value)?;
            self.chunk.push(Op::MapInsert, key.span);
        }
        Ok(())
    }

    /// Writes a struct literal, one field after another.
    fn compile_struct_literal(
        &mut self,
        fields: &[FieldInit],
        index: Option<usize>,
        span: Span,
    ) -> Result<(), SomoniError> {
        self.chunk
            .push(Op::NewStruct(filled(index, "struct of literal")?), span);
        for init in fields {
            self.compile_expr(&init.value)?;
            let position = filled(init.index, "field position")?;
            self.chunk.push(Op::InitField(position), span);
        }
        Ok(())
    }

    /// Pushes an enum value as a constant.
    fn push_enum(&mut self, decl: &Rc<EnumDecl>, variant: usize, span: Span) {
        let value = EnumValue {
            decl: Rc::clone(decl),
            variant,
        };
        self.push_constant(Value::Enum(Rc::new(value)), span);
    }

    /// Writes expressions from left to right, leaving their values on the stack.
    fn compile_args(&mut self, args: &[Expr]) -> Result<(), SomoniError> {
        for arg in args {
            self.compile_expr(arg)?;
        }
        Ok(())
    }

    /// Adds a constant and the instruction that pushes it.
    fn push_constant(&mut self, value: Value, span: Span) {
        let i = self.chunk.constant(value);
        self.chunk.push(Op::Push(i), span);
    }

    /// Makes room for a variable slot and returns it.
    fn use_slot(&mut self, slot: usize) -> usize {
        self.chunk.locals = self.chunk.locals.max(slot + 1);
        slot
    }
}
