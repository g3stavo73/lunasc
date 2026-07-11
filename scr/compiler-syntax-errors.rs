use std::fmt;

#[derive(Debug, Clone)]
pub enum LunaError {
    UnexpectedChar { ch: char, line: usize, col: usize },
    UnterminatedString { line: usize, col: usize },
    InvalidEscapeSequence { ch: char, line: usize, col: usize },
    UnexpectedEof,
    UnexpectedToken { expected: String, found: String, line: usize, col: usize },
    InvalidSyntax { message: String, line: usize, col: usize },
    MissingReturnType { fn_name: String, line: usize, col: usize },
    UndeclaredVariable { name: String, line: usize, col: usize },
    TypeMismatch { expected: String, found: String, line: usize, col: usize },
    DuplicateDeclaration { name: String, line: usize, col: usize },
    ImmutableAssignment { name: String, line: usize, col: usize },
    UndeclaredFunction { name: String, line: usize, col: usize },
    UndeclaredClass { name: String, line: usize, col: usize },
    WrongArgumentCount { fn_name: String, expected: usize, found: usize, line: usize, col: usize },
    ReturnOutsideFunction { line: usize, col: usize },
    InvalidOperandTypes { op: String, left: String, right: String, line: usize, col: usize },
    RuntimeError { message: String },
    DivisionByZero { line: usize, col: usize },
    StackOverflow { fn_name: String },
    UndefinedMethod { class_name: String, method: String },
    UndefinedProperty { class_name: String, property: String },
    NullDereference { line: usize, col: usize },
    ModuleNotFound { module: String },
    CircularImport { module: String },
    CodegenError { message: String },
}

impl fmt::Display for LunaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LunaError::UnexpectedChar { ch, line, col } => {
                write!(f, "error[E001] unexpected character `{ch}` at {line}:{col}")
            }
            LunaError::UnterminatedString { line, col } => {
                write!(f, "error[E002] unterminated string literal at {line}:{col}")
            }
            LunaError::InvalidEscapeSequence { ch, line, col } => {
                write!(f, "error[E003] invalid escape sequence `\\{ch}` at {line}:{col}")
            }
            LunaError::UnexpectedEof => {
                write!(f, "error[E004] unexpected end of file")
            }
            LunaError::UnexpectedToken { expected, found, line, col } => {
                write!(f, "error[E010] expected `{expected}`, found `{found}` at {line}:{col}")
            }
            LunaError::InvalidSyntax { message, line, col } => {
                write!(f, "error[E011] syntax error at {line}:{col}: {message}")
            }
            LunaError::MissingReturnType { fn_name, line, col } => {
                write!(f, "error[E012] function `{fn_name}` is missing a return type annotation at {line}:{col}")
            }
            LunaError::UndeclaredVariable { name, line, col } => {
                write!(f, "error[E020] use of undeclared variable `{name}` at {line}:{col}")
            }
            LunaError::TypeMismatch { expected, found, line, col } => {
                write!(f, "error[E021] type mismatch at {line}:{col}: expected `{expected}`, found `{found}`")
            }
            LunaError::DuplicateDeclaration { name, line, col } => {
                write!(f, "error[E022] `{name}` is already declared in this scope at {line}:{col}")
            }
            LunaError::ImmutableAssignment { name, line, col } => {
                write!(f, "error[E023] cannot assign to immutable variable `{name}` at {line}:{col}\n  help: declare as `let mut {name}` to allow mutation")
            }
            LunaError::UndeclaredFunction { name, line, col } => {
                write!(f, "error[E024] call to undeclared function `{name}` at {line}:{col}")
            }
            LunaError::UndeclaredClass { name, line, col } => {
                write!(f, "error[E025] reference to undeclared class `{name}` at {line}:{col}")
            }
            LunaError::WrongArgumentCount { fn_name, expected, found, line, col } => {
                write!(f, "error[E026] `{fn_name}` expects {expected} argument(s), but {found} were provided at {line}:{col}")
            }
            LunaError::ReturnOutsideFunction { line, col } => {
                write!(f, "error[E027] `return` statement outside of a function at {line}:{col}")
            }
            LunaError::InvalidOperandTypes { op, left, right, line, col } => {
                write!(f, "error[E028] operator `{op}` cannot be applied to types `{left}` and `{right}` at {line}:{col}")
            }
            LunaError::RuntimeError { message } => {
                write!(f, "error[E030] runtime error: {message}")
            }
            LunaError::DivisionByZero { line, col } => {
                write!(f, "error[E031] division by zero at {line}:{col}")
            }
            LunaError::StackOverflow { fn_name } => {
                write!(f, "error[E032] stack overflow in recursive call to `{fn_name}`")
            }
            LunaError::UndefinedMethod { class_name, method } => {
                write!(f, "error[E033] method `{method}` not found on class `{class_name}`")
            }
            LunaError::UndefinedProperty { class_name, property } => {
                write!(f, "error[E034] property `{property}` not found on class `{class_name}`")
            }
            LunaError::NullDereference { line, col } => {
                write!(f, "error[E035] null dereference at {line}:{col}")
            }
            LunaError::ModuleNotFound { module } => {
                write!(f, "error[E040] module `{module}` not found")
            }
            LunaError::CircularImport { module } => {
                write!(f, "error[E041] circular import detected for module `{module}`")
            }
            LunaError::CodegenError { message } => {
                write!(f, "error[E050] code generation error: {message}")
            }
        }
    }
}

pub type LunaResult<T> = Result<T, LunaError>;

pub fn format_error_with_context(err: &LunaError, source: &str) -> String {
    let mut out = format!("{err}\n");
    let (line, col) = match err {
        LunaError::UnexpectedChar { line, col, .. }
        | LunaError::UnterminatedString { line, col }
        | LunaError::InvalidEscapeSequence { line, col, .. }
        | LunaError::UnexpectedToken { line, col, .. }
        | LunaError::InvalidSyntax { line, col, .. }
        | LunaError::MissingReturnType { line, col, .. }
        | LunaError::UndeclaredVariable { line, col, .. }
        | LunaError::TypeMismatch { line, col, .. }
        | LunaError::DuplicateDeclaration { line, col, .. }
        | LunaError::ImmutableAssignment { line, col, .. }
        | LunaError::UndeclaredFunction { line, col, .. }
        | LunaError::UndeclaredClass { line, col, .. }
        | LunaError::WrongArgumentCount { line, col, .. }
        | LunaError::ReturnOutsideFunction { line, col }
        | LunaError::InvalidOperandTypes { line, col, .. }
        | LunaError::DivisionByZero { line, col }
        | LunaError::NullDereference { line, col } => (*line, *col),
        _ => return out,
    };

    let lines: Vec<&str> = source.lines().collect();
    if line > 0 && line <= lines.len() {
        let src_line = lines[line - 1];
        out.push_str(&format!("   --> line {line}, col {col}\n"));
        out.push_str(&format!("    |\n"));
        out.push_str(&format!("{line:4} | {src_line}\n"));
        out.push_str(&format!("    | {}{}\n", " ".repeat(col.saturating_sub(1)), "^"));
    }
    out
}
