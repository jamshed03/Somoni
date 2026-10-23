use std::fmt;

use crate::span::Span;
use crate::token::Token;
use crate::types::Type;

/// In which part of the pipeline the error happened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    Lex,
    Parse,
    Import,
    Type,
    Runtime,
}

impl ErrorKind {
    /// Returns how the kind starts a message, e.g. `Type error`.
    fn label(self) -> &'static str {
        match self {
            ErrorKind::Lex => "Lex error",
            ErrorKind::Parse => "Parse error",
            ErrorKind::Import => "Import error",
            ErrorKind::Type => "Type error",
            ErrorKind::Runtime => "Runtime error",
        }
    }
}

/// All errors Somoni knows. Each has a code like `T001` and a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    // lexer
    UnexpectedCharacter {
        found: char,
    },
    UnterminatedString,
    UnterminatedComment,
    IntegerTooLarge {
        text: String,
    },
    InvalidFloat {
        text: String,
    },

    // parser
    Expected {
        what: String,
    },
    UnexpectedToken {
        found: Token,
    },
    BlockNotClosed,
    ForStartConst,
    FnNotTopLevel,
    StructNotTopLevel,
    EnumNotTopLevel,
    InvalidAssignTarget,
    ImportNotFirst,
    ExportNotTopLevel,
    ClassNotTopLevel,
    DuplicateConstructor,
    InterfaceNotTopLevel,
    ReadonlyNotAllowed,
    StaticNotAllowed,
    AbstractNotAllowed,
    PrivateAbstract,

    // loader and imports
    CannotReadFile {
        name: String,
    },
    ImportCycle {
        cycle: String,
    },
    OnlyDeclarations,
    ReservedPath {
        path: String,
    },
    NotExported {
        name: String,
        path: String,
    },
    NotInFile {
        name: String,
        path: String,
    },
    ImportConflict {
        name: String,
    },

    // type checker
    UnknownType {
        name: String,
    },
    UnknownVariable {
        name: String,
    },
    CannotAssign {
        found: Type,
        expected: Type,
    },
    AlreadyDeclared {
        name: String,
    },
    AssignToConst {
        name: String,
    },
    OperandTypesDiffer {
        left: Type,
        right: Type,
    },
    OperatorNotAllowed {
        found: Type,
    },
    NotNeedsBool {
        found: Type,
    },
    NegNeedsNumber {
        found: Type,
    },
    ConditionNotBool {
        found: Type,
    },
    OutsideLoop {
        keyword: &'static str,
    },
    UnknownFunction {
        name: String,
    },
    WrongArgCount {
        name: String,
        expected: usize,
        found: usize,
    },
    WrongArgType {
        name: String,
        found: Type,
    },
    DuplicateFunction {
        name: String,
    },
    BuiltinRedeclared {
        name: String,
    },
    ReturnOutsideFunction,
    ReturnType {
        found: Type,
        expected: Type,
    },
    MissingReturn {
        name: String,
        expected: Type,
    },
    ListElementType {
        expected: Type,
        found: Type,
    },
    IndexNotInt {
        found: Type,
    },
    NotIndexable {
        found: Type,
    },
    TypeArguments {
        name: String,
        expected: usize,
        found: usize,
    },
    EmptyListType,
    DuplicateType {
        name: String,
    },
    DuplicateField {
        name: String,
    },
    UnknownField {
        struct_name: String,
        name: String,
    },
    MissingField {
        struct_name: String,
        name: String,
    },
    NoFields {
        found: Type,
    },
    StructContainsItself {
        name: String,
    },
    NullNotAllowed {
        expected: Type,
    },
    NullTypeUnknown,
    OptionalVoid,
    MapKeyType {
        found: Type,
    },
    EmptyMapType,
    MapEntryType {
        part: &'static str,
        expected: Type,
        found: Type,
    },
    MapIndexType {
        expected: Type,
        found: Type,
    },
    NotIterable {
        found: Type,
    },
    UnknownVariant {
        enum_name: String,
        name: String,
    },
    MatchNotEnum {
        found: Type,
    },
    PatternWrongEnum {
        expected: Type,
        found: String,
    },
    DuplicateBranch {
        name: String,
    },
    RestNotLast,
    RestUnreachable,
    MissingBranch {
        missing: String,
    },
    DuplicateVariant {
        name: String,
    },
    EmptyEnum,
    ResultTypeUnknown {
        name: &'static str,
    },
    FieldNotSet {
        class: String,
        field: String,
    },
    ClassLiteral {
        name: String,
    },
    NotAClass {
        name: String,
    },
    ThisOutsideClass,
    DuplicateMember {
        class: String,
        name: String,
    },
    UnknownMethod {
        class: String,
        name: String,
    },
    NoMethods {
        found: Type,
    },
    PrivateMember {
        class: String,
        name: String,
    },
    MissingInterfaceMethod {
        interface: String,
        method: String,
    },
    InterfaceMethodType {
        interface: String,
        method: String,
    },
    PrivateInterfaceMethod {
        interface: String,
        method: String,
    },
    NotAnInterface {
        name: String,
    },
    InterfaceNew {
        name: String,
    },
    ExtendsNotClass {
        name: String,
    },
    InheritanceCycle {
        name: String,
    },
    MissingOverride {
        method: String,
        parent: String,
    },
    NothingToOverride {
        method: String,
    },
    OverrideMismatch {
        parent: String,
        method: String,
    },
    MissingSuper {
        class: String,
    },
    SuperInitMisplaced,
    SuperOutsideSubclass,
    ProtectedMember {
        class: String,
        name: String,
    },
    ReadonlyField {
        class: String,
        name: String,
    },
    NotStatic {
        class: String,
        name: String,
    },
    AbstractNew {
        name: String,
    },
    AbstractNotOverridden {
        class: String,
        method: String,
    },
    AbstractSuper {
        class: String,
        name: String,
    },
    StaticOnObject {
        class: String,
        name: String,
    },
    ThisBeforeFieldsSet {
        missing: String,
    },
    BoundVarType {
        name: String,
        expected: Type,
        found: Type,
    },

    // runtime
    Internal {
        message: String,
    },
    DivisionByZero,
    IntegerOverflow,
    RecursionTooDeep {
        limit: usize,
    },
    IndexOutOfRange {
        index: i64,
        length: usize,
    },
    KeyNotFound {
        key: String,
    },
    DuplicateKey {
        key: String,
    },
}

impl ErrorCode {
    /// Returns the code of the error, e.g. `T001`.
    pub fn code(&self) -> &'static str {
        match self {
            ErrorCode::UnexpectedCharacter { .. } => "L001",
            ErrorCode::UnterminatedString => "L002",
            ErrorCode::UnterminatedComment => "L003",
            ErrorCode::IntegerTooLarge { .. } => "L004",
            ErrorCode::InvalidFloat { .. } => "L005",

            ErrorCode::Expected { .. } => "P001",
            ErrorCode::UnexpectedToken { .. } => "P002",
            ErrorCode::BlockNotClosed => "P003",
            ErrorCode::ForStartConst => "P004",
            ErrorCode::FnNotTopLevel => "P005",
            ErrorCode::StructNotTopLevel => "P006",
            ErrorCode::EnumNotTopLevel => "P008",
            ErrorCode::InvalidAssignTarget => "P007",
            ErrorCode::ImportNotFirst => "P009",
            ErrorCode::ExportNotTopLevel => "P010",
            ErrorCode::ClassNotTopLevel => "P011",
            ErrorCode::DuplicateConstructor => "P012",
            ErrorCode::InterfaceNotTopLevel => "P014",
            ErrorCode::ReadonlyNotAllowed => "P015",
            ErrorCode::StaticNotAllowed => "P016",
            ErrorCode::AbstractNotAllowed => "P017",
            ErrorCode::PrivateAbstract => "P018",

            ErrorCode::CannotReadFile { .. } => "M001",
            ErrorCode::ImportCycle { .. } => "M002",
            ErrorCode::OnlyDeclarations => "M003",
            ErrorCode::ReservedPath { .. } => "M004",
            ErrorCode::NotExported { .. } => "M005",
            ErrorCode::NotInFile { .. } => "M006",
            ErrorCode::ImportConflict { .. } => "M007",

            ErrorCode::UnknownType { .. } => "T001",
            ErrorCode::UnknownVariable { .. } => "T002",
            ErrorCode::CannotAssign { .. } => "T003",
            ErrorCode::AlreadyDeclared { .. } => "T004",
            ErrorCode::AssignToConst { .. } => "T005",
            ErrorCode::OperandTypesDiffer { .. } => "T006",
            ErrorCode::OperatorNotAllowed { .. } => "T007",
            ErrorCode::NotNeedsBool { .. } => "T008",
            ErrorCode::NegNeedsNumber { .. } => "T009",
            ErrorCode::ConditionNotBool { .. } => "T010",
            ErrorCode::OutsideLoop { .. } => "T011",
            ErrorCode::UnknownFunction { .. } => "T012",
            ErrorCode::WrongArgCount { .. } => "T013",
            ErrorCode::WrongArgType { .. } => "T014",
            ErrorCode::DuplicateFunction { .. } => "T015",
            ErrorCode::BuiltinRedeclared { .. } => "T016",
            ErrorCode::ReturnOutsideFunction => "T017",
            ErrorCode::ReturnType { .. } => "T018",
            ErrorCode::MissingReturn { .. } => "T019",
            ErrorCode::ListElementType { .. } => "T020",
            ErrorCode::IndexNotInt { .. } => "T021",
            ErrorCode::NotIndexable { .. } => "T022",
            ErrorCode::TypeArguments { .. } => "T023",
            ErrorCode::EmptyListType => "T024",
            ErrorCode::DuplicateType { .. } => "T025",
            ErrorCode::DuplicateField { .. } => "T026",
            ErrorCode::UnknownField { .. } => "T027",
            ErrorCode::MissingField { .. } => "T028",
            ErrorCode::NoFields { .. } => "T029",
            ErrorCode::StructContainsItself { .. } => "T030",
            ErrorCode::NullNotAllowed { .. } => "T031",
            ErrorCode::NullTypeUnknown => "T032",
            ErrorCode::OptionalVoid => "T033",
            ErrorCode::MapKeyType { .. } => "T034",
            ErrorCode::EmptyMapType => "T035",
            ErrorCode::MapEntryType { .. } => "T036",
            ErrorCode::MapIndexType { .. } => "T037",
            ErrorCode::NotIterable { .. } => "T038",
            ErrorCode::BoundVarType { .. } => "T039",
            ErrorCode::UnknownVariant { .. } => "T040",
            ErrorCode::MatchNotEnum { .. } => "T041",
            ErrorCode::PatternWrongEnum { .. } => "T042",
            ErrorCode::DuplicateBranch { .. } => "T043",
            ErrorCode::RestNotLast => "T044",
            ErrorCode::RestUnreachable => "T045",
            ErrorCode::MissingBranch { .. } => "T046",
            ErrorCode::DuplicateVariant { .. } => "T047",
            ErrorCode::EmptyEnum => "T048",
            ErrorCode::ResultTypeUnknown { .. } => "T049",
            ErrorCode::FieldNotSet { .. } => "T050",
            ErrorCode::ClassLiteral { .. } => "T051",
            ErrorCode::NotAClass { .. } => "T052",
            ErrorCode::ThisOutsideClass => "T053",
            ErrorCode::ThisBeforeFieldsSet { .. } => "T054",
            ErrorCode::DuplicateMember { .. } => "T055",
            ErrorCode::UnknownMethod { .. } => "T056",
            ErrorCode::NoMethods { .. } => "T057",
            ErrorCode::PrivateMember { .. } => "T058",
            ErrorCode::MissingInterfaceMethod { .. } => "T059",
            ErrorCode::InterfaceMethodType { .. } => "T060",
            ErrorCode::PrivateInterfaceMethod { .. } => "T061",
            ErrorCode::NotAnInterface { .. } => "T062",
            ErrorCode::InterfaceNew { .. } => "T063",
            ErrorCode::ExtendsNotClass { .. } => "T064",
            ErrorCode::InheritanceCycle { .. } => "T065",
            ErrorCode::MissingOverride { .. } => "T066",
            ErrorCode::NothingToOverride { .. } => "T067",
            ErrorCode::OverrideMismatch { .. } => "T068",
            ErrorCode::MissingSuper { .. } => "T069",
            ErrorCode::SuperInitMisplaced => "T070",
            ErrorCode::SuperOutsideSubclass => "T071",
            ErrorCode::ProtectedMember { .. } => "T072",
            ErrorCode::ReadonlyField { .. } => "T073",
            ErrorCode::NotStatic { .. } => "T074",
            ErrorCode::StaticOnObject { .. } => "T075",
            ErrorCode::AbstractNew { .. } => "T076",
            ErrorCode::AbstractNotOverridden { .. } => "T077",
            ErrorCode::AbstractSuper { .. } => "T078",

            ErrorCode::Internal { .. } => "R000",
            ErrorCode::DivisionByZero => "R001",
            ErrorCode::IntegerOverflow => "R002",
            ErrorCode::RecursionTooDeep { .. } => "R004",
            ErrorCode::IndexOutOfRange { .. } => "R005",
            ErrorCode::KeyNotFound { .. } => "R006",
            ErrorCode::DuplicateKey { .. } => "R007",
        }
    }

    /// Returns the part of the pipeline from the first letter of the code: `L`, `P`, `M`, `T` or `R`.
    pub fn kind(&self) -> ErrorKind {
        match self.code().chars().next() {
            Some('L') => ErrorKind::Lex,
            Some('P') => ErrorKind::Parse,
            Some('M') => ErrorKind::Import,
            Some('T') => ErrorKind::Type,
            _ => ErrorKind::Runtime,
        }
    }

    /// Returns the message, in lowercase, e.g. `unknown variable 'y'`.
    pub fn message(&self) -> String {
        match self {
            ErrorCode::UnexpectedCharacter { found } => format!("unexpected character '{}'", found),
            ErrorCode::UnterminatedString => "unterminated string".to_string(),
            ErrorCode::UnterminatedComment => "unterminated comment".to_string(),
            ErrorCode::IntegerTooLarge { text } => format!("integer too large '{}'", text),
            ErrorCode::InvalidFloat { text } => format!("invalid float '{}'", text),

            ErrorCode::Expected { what } => format!("expected {}", what),
            ErrorCode::UnexpectedToken { found } => format!("unexpected token {:?}", found),
            ErrorCode::BlockNotClosed => "block is not closed, expected '}'".to_string(),
            ErrorCode::ForStartConst => "for loop start must be 'var', not 'const'".to_string(),
            ErrorCode::FnNotTopLevel => {
                "functions can only be declared at the top level".to_string()
            }
            ErrorCode::StructNotTopLevel => {
                "structs can only be declared at the top level".to_string()
            }
            ErrorCode::EnumNotTopLevel => "enums can only be declared at the top level".to_string(),
            ErrorCode::ImportNotFirst => {
                "'import' is only allowed at the start of a file, before all other statements"
                    .to_string()
            }
            ErrorCode::ExportNotTopLevel => "'export' is only allowed at the top level".to_string(),
            ErrorCode::ClassNotTopLevel => {
                "classes can only be declared at the top level".to_string()
            }
            ErrorCode::DuplicateConstructor => "a class can only have one constructor".to_string(),
            ErrorCode::InterfaceNotTopLevel => {
                "interfaces can only be declared at the top level".to_string()
            }
            ErrorCode::AbstractNotAllowed => "'abstract' is only allowed before a class and before methods of an abstract class without 'static' or 'override'".to_string(),
            ErrorCode::PrivateAbstract => {
                "an abstract method cannot be private, because subclasses must override it".to_string()
            }
            ErrorCode::StaticNotAllowed => {
                "'static' is only allowed before a method without 'override'".to_string()
            }
            ErrorCode::ReadonlyNotAllowed => {
                "'readonly' is only allowed before a field of a class".to_string()
            }
            ErrorCode::InvalidAssignTarget => {
                "can only assign to a variable, a list element or a field".to_string()
            }

            ErrorCode::CannotReadFile { name } => format!("cannot read file '{}'", name),
            ErrorCode::ImportCycle { cycle } => format!("files import each other: {}", cycle),
            ErrorCode::OnlyDeclarations => {
                "an imported file may only contain fn, struct, enum, class, interface and import"
                    .to_string()
            }
            ErrorCode::ReservedPath { path } => format!(
                "'{}': paths starting with '/' or '@' are not supported yet, use a path relative to this file",
                path
            ),
            ErrorCode::NotExported { name, path } => {
                format!(
                    "'{}' in '{}' is not exported (add 'export' there)",
                    name, path
                )
            }
            ErrorCode::NotInFile { name, path } => {
                format!("'{}' has no function, struct or enum '{}'", path, name)
            }
            ErrorCode::ImportConflict { name } => {
                format!("'{}' is already imported in this file", name)
            }

            ErrorCode::UnknownType { name } => format!("unknown type '{}'", name),
            ErrorCode::UnknownVariable { name } => format!("unknown variable '{}'", name),
            ErrorCode::CannotAssign { found, expected } => {
                format!("cannot assign {} to {}", found, expected)
            }
            ErrorCode::AlreadyDeclared { name } => {
                format!("'{}' is already declared in this block", name)
            }
            ErrorCode::AssignToConst { name } => format!("cannot assign to const '{}'", name),
            ErrorCode::OperandTypesDiffer { left, right } => {
                format!("cannot use operator on {} and {}", left, right)
            }
            ErrorCode::OperatorNotAllowed { found } => {
                format!("operator not allowed for {}", found)
            }
            ErrorCode::NotNeedsBool { found } => format!("'!' needs bool, got {}", found),
            ErrorCode::NegNeedsNumber { found } => {
                format!("'-' needs int or float, got {}", found)
            }
            ErrorCode::ConditionNotBool { found } => {
                format!("condition must be bool, got {}", found)
            }
            ErrorCode::OutsideLoop { keyword } => format!("'{}' outside of a loop", keyword),
            ErrorCode::UnknownFunction { name } => format!("unknown function '{}'", name),
            ErrorCode::WrongArgCount {
                name,
                expected,
                found,
            } => {
                let word = if *expected == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                format!("'{}' expects {} {}, got {}", name, expected, word, found)
            }
            ErrorCode::WrongArgType { name, found } => {
                format!("cannot use {} as argument of '{}'", found, name)
            }
            ErrorCode::DuplicateFunction { name } => {
                format!("function '{}' is already declared", name)
            }
            ErrorCode::BuiltinRedeclared { name } => format!("'{}' is a built-in function", name),
            ErrorCode::ReturnOutsideFunction => "'return' outside of a function".to_string(),
            ErrorCode::ReturnType { found, expected } => {
                format!("function must return {}, got {}", expected, found)
            }
            ErrorCode::MissingReturn { name, expected } => format!(
                "function '{}' does not return {} on every path",
                name, expected
            ),
            ErrorCode::ListElementType { expected, found } => {
                format!("list elements must all be {}, got {}", expected, found)
            }
            ErrorCode::IndexNotInt { found } => format!("list index must be int, got {}", found),
            ErrorCode::NotIndexable { found } => format!("cannot use [] on {}", found),
            ErrorCode::TypeArguments {
                name,
                expected,
                found,
            } => {
                let word = if *expected == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                format!(
                    "'{}' expects {} type {}, got {}",
                    name, expected, word, found
                )
            }
            ErrorCode::EmptyListType => "the type of this empty list is unknown here".to_string(),
            ErrorCode::DuplicateType { name } => format!("type '{}' already exists", name),
            ErrorCode::DuplicateField { name } => {
                format!("field '{}' appears more than once", name)
            }
            ErrorCode::UnknownField { struct_name, name } => {
                format!("'{}' has no field '{}'", struct_name, name)
            }
            ErrorCode::MissingField { struct_name, name } => {
                format!("missing field '{}' of '{}'", name, struct_name)
            }
            ErrorCode::NoFields { found } => format!("cannot use '.' on {}", found),
            ErrorCode::NullNotAllowed { expected } => format!(
                "null is not allowed for {}, only for types with '?' like {}?",
                expected, expected
            ),
            ErrorCode::NullTypeUnknown => "the type of this null is unknown here".to_string(),
            ErrorCode::OptionalVoid => "'void' cannot be optional".to_string(),
            ErrorCode::MapKeyType { found } => {
                format!("map keys must be string or int, got {}", found)
            }
            ErrorCode::EmptyMapType => "the type of this empty map is unknown here".to_string(),
            ErrorCode::MapEntryType {
                part,
                expected,
                found,
            } => format!("map {} must all be {}, got {}", part, expected, found),
            ErrorCode::MapIndexType { expected, found } => {
                format!("the key of this map must be {}, got {}", expected, found)
            }
            ErrorCode::UnknownVariant { enum_name, name } => {
                format!("'{}' has no variant '{}'", enum_name, name)
            }
            ErrorCode::MatchNotEnum { found } => {
                format!("match needs an enum or a result, got {}", found)
            }
            ErrorCode::PatternWrongEnum { expected, found } => {
                format!("this match is over {}, not {}", expected, found)
            }
            ErrorCode::DuplicateBranch { name } => {
                format!("there is already a branch for {}", name)
            }
            ErrorCode::RestNotLast => "'_' must be the last branch".to_string(),
            ErrorCode::RestUnreachable => {
                "all variants already have a branch, so '_' would never run".to_string()
            }
            ErrorCode::MissingBranch { missing } => {
                format!("no branch for {} (add it or use '_')", missing)
            }
            ErrorCode::DuplicateVariant { name } => {
                format!("variant '{}' appears more than once", name)
            }
            ErrorCode::EmptyEnum => "an enum needs at least one variant".to_string(),
            ErrorCode::ResultTypeUnknown { name } => {
                format!("the type of this {}(...) is unknown here", name)
            }
            ErrorCode::NotIterable { found } => {
                format!("cannot loop over {} (only list, map or string)", found)
            }
            ErrorCode::FieldNotSet { class, field } => format!(
                "field '{}' of '{}' gets no value (give it a start value or set it in the constructor)",
                field, class
            ),
            ErrorCode::ClassLiteral { name } => {
                format!("'{}' is a class, create it with 'new {}(...)'", name, name)
            }
            ErrorCode::NotAClass { name } => format!(
                "'{}' is a struct, not a class: create it with '{} {{ ... }}'",
                name, name
            ),
            ErrorCode::ThisOutsideClass => {
                "'this' only exists in the constructor and the non-static methods of a class"
                    .to_string()
            }
            ErrorCode::ThisBeforeFieldsSet { missing } => format!(
                "'this' is used before every field has a value (still missing: {})",
                missing
            ),
            ErrorCode::DuplicateMember { class, name } => {
                format!("'{}' is already a field or method of '{}'", name, class)
            }
            ErrorCode::UnknownMethod { class, name } => {
                format!("'{}' has no method '{}'", class, name)
            }
            ErrorCode::NoMethods { found } => format!(
                "cannot call a method on {} (only objects of classes have methods)",
                found
            ),
            ErrorCode::PrivateMember { class, name } => format!(
                "'{}' is private in '{}' and can only be used inside the class",
                name, class
            ),
            ErrorCode::MissingInterfaceMethod { interface, method } => format!(
                "this class implements '{}' but has no method '{}'",
                interface, method
            ),
            ErrorCode::InterfaceMethodType { interface, method } => {
                format!(
                    "to implement '{}', this method must be {}",
                    interface, method
                )
            }
            ErrorCode::PrivateInterfaceMethod { interface, method } => format!(
                "'{}' implements '{}', so it must be public",
                method, interface
            ),
            ErrorCode::NotAnInterface { name } => format!("'{}' is not an interface", name),
            ErrorCode::InterfaceNew { name } => format!(
                "'{}' is an interface and has no objects of its own, create an object of a class that implements it",
                name
            ),
            ErrorCode::ExtendsNotClass { name } => {
                format!("'{}' is not a class, only a class can be extended", name)
            }
            ErrorCode::InheritanceCycle { name } => {
                format!("'{}' inherits from itself", name)
            }
            ErrorCode::MissingOverride { method, parent } => format!(
                "'{}' is inherited from '{}', write 'override' in front to replace it",
                method, parent
            ),
            ErrorCode::NothingToOverride { method } => {
                format!("there is no inherited method '{}' to override", method)
            }
            ErrorCode::OverrideMismatch { parent, method } => {
                format!(
                    "to override it, this method must be {} like in '{}'",
                    method, parent
                )
            }
            ErrorCode::MissingSuper { class } => format!(
                "the constructor of '{}' must start with super(...), because it extends another class",
                class
            ),
            ErrorCode::SuperInitMisplaced => {
                "super(...) is only allowed as the first statement in the constructor of a subclass"
                    .to_string()
            }
            ErrorCode::SuperOutsideSubclass => {
                "'super' only works in a class that extends another class".to_string()
            }
            ErrorCode::ProtectedMember { class, name } => format!(
                "'{}' is protected in '{}' and can only be used in it and its subclasses",
                name, class
            ),
            ErrorCode::ReadonlyField { class, name } => format!(
                "'{}' is readonly in '{}' and can only be set in its constructor",
                name, class
            ),
            ErrorCode::NotStatic { class, name } => {
                format!(
                    "'{}' is not static in '{}', call it on an object",
                    name, class
                )
            }
            ErrorCode::AbstractNotOverridden { class, method } => format!(
                "'{}' of '{}' is abstract and must be overridden here",
                method, class
            ),
            ErrorCode::AbstractSuper { class, name } => format!(
                "'{}' is abstract in '{}' and has no body to call with 'super'",
                name, class
            ),
            ErrorCode::AbstractNew { name } => format!(
                "'{}' is abstract and has no objects, create one of a subclass",
                name
            ),
            ErrorCode::StaticOnObject { class, name } => format!(
                "'{}' is static, call it as '{}::{}(...)'",
                name, class, name
            ),
            ErrorCode::BoundVarType {
                name,
                expected,
                found,
            } => format!(
                "variable '{}' must be {} here, got {}",
                name, expected, found
            ),
            ErrorCode::StructContainsItself { name } => {
                format!("'{}' contains itself, so no value of it can be built", name)
            }

            ErrorCode::Internal { message } => format!("internal error: {}", message),
            ErrorCode::DivisionByZero => "division by zero".to_string(),
            ErrorCode::IntegerOverflow => "integer overflow".to_string(),
            ErrorCode::RecursionTooDeep { limit } => {
                format!("recursion too deep (more than {} nested calls)", limit)
            }
            ErrorCode::IndexOutOfRange { index, length } => format!(
                "index {} is out of range (list has {} elements)",
                index, length
            ),
            ErrorCode::KeyNotFound { key } => {
                format!("key {} is not in the map (check with has first)", key)
            }
            ErrorCode::DuplicateKey { key } => {
                format!("key {} appears twice in the map literal", key)
            }
        }
    }
}

/// An error with its place in the source (None if the place is not known yet).
#[derive(Debug, Clone, PartialEq)]
pub struct SomoniError {
    pub code: ErrorCode,
    pub span: Option<Span>,
}

impl SomoniError {
    /// An error at this place.
    pub fn at(span: Span, code: ErrorCode) -> SomoniError {
        SomoniError {
            code,
            span: Some(span),
        }
    }

    /// Error R000: something the type checker should have prevented. This is a bug in Somoni.
    pub fn internal(message: impl Into<String>) -> SomoniError {
        ErrorCode::Internal {
            message: message.into(),
        }
        .into()
    }

    /// Returns the message with the line of `source` it is in and `^^^` under the place.
    pub fn render(&self, file_name: Option<&str>, source: &str) -> String {
        let header = self.header(file_name);
        let Some(span) = self.span else {
            return header;
        };
        let Some(text) = source.lines().nth(span.line - 1) else {
            return header;
        };
        let number = span.line.to_string();
        let gutter = " ".repeat(number.len());
        let marker = marker(span, text);
        format!("{header}\n  {number} | {text}\n  {gutter} | {marker}")
    }

    /// Sets the place if the error has none yet.
    pub fn with_span_if_missing(mut self, span: Option<Span>) -> SomoniError {
        if self.span.is_none() {
            self.span = span;
        }
        self
    }

    /// The first line of the message, e.g. `Type error [T002]: mathe.smn: Line 4: unknown variable 'y'`.
    fn header(&self, file_name: Option<&str>) -> String {
        let kind = self.code.kind().label();
        let place = match (file_name, self.span) {
            (Some(name), Some(span)) => format!("{}: Line {}: ", name, span.line),
            (None, Some(span)) => format!("Line {}: ", span.line),
            (_, None) => String::new(),
        };
        format!(
            "{} [{}]: {}{}",
            kind,
            self.code.code(),
            place,
            self.code.message()
        )
    }
}

/// Returns the `^^^` under the place in the line `text`, indented with the same tabs as the line.
fn marker(span: Span, text: &str) -> String {
    let end_col = if span.end_line == span.line {
        span.end_col
    } else {
        text.chars().count() + 1
    };
    let width = end_col.saturating_sub(span.col).max(1);
    let indent: String = text
        .chars()
        .take(span.col - 1)
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect();
    format!("{}{}", indent, "^".repeat(width))
}

/// Turns an error code into an error without a place (used for runtime errors).
impl From<ErrorCode> for SomoniError {
    fn from(code: ErrorCode) -> SomoniError {
        SomoniError { code, span: None }
    }
}

/// The first line of the message, without a file name.
impl fmt::Display for SomoniError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.header(None))
    }
}
