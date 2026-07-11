use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum LunaType {
    String,
    Number,
    Bool,
    Void,
    I32,
    I64,
    F32,
    F64,
    U8,
    Usize,
    Custom(std::string::String),
    Generic(std::string::String, Vec<LunaType>),
    Array(Box<LunaType>),
    Function(Vec<LunaType>, Box<LunaType>),
    Option(Box<LunaType>),
    Inferred,
}

impl LunaType {
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            LunaType::Number
                | LunaType::I32
                | LunaType::I64
                | LunaType::F32
                | LunaType::F64
                | LunaType::U8
                | LunaType::Usize
        )
    }

    pub fn display(&self) -> std::string::String {
        self.to_string()
    }
}

impl fmt::Display for LunaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LunaType::String => write!(f, "string"),
            LunaType::Number => write!(f, "number"),
            LunaType::Bool => write!(f, "bool"),
            LunaType::Void => write!(f, "void"),
            LunaType::I32 => write!(f, "i32"),
            LunaType::I64 => write!(f, "i64"),
            LunaType::F32 => write!(f, "f32"),
            LunaType::F64 => write!(f, "f64"),
            LunaType::U8 => write!(f, "u8"),
            LunaType::Usize => write!(f, "usize"),
            LunaType::Custom(n) => write!(f, "{n}"),
            LunaType::Generic(n, args) => {
                let args_str: Vec<_> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{n}<{}>", args_str.join(", "))
            }
            LunaType::Array(inner) => write!(f, "[{inner}]"),
            LunaType::Function(ps, r) => {
                let ps_str: Vec<_> = ps.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({}) -> {r}", ps_str.join(", "))
            }
            LunaType::Option(inner) => write!(f, "{inner}?"),
            LunaType::Inferred => write!(f, "_"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Not,
    Negate,
    BitNot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompoundOp {
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: std::string::String,
    pub ty: LunaType,
    pub default: Option<Expr>,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub enum Expr {
    StringLit {
        value: std::string::String,
        line: usize,
        col: usize,
    },
    FStringLit {
        template: std::string::String,
        line: usize,
        col: usize,
    },
    NumberLit {
        value: f64,
        line: usize,
        col: usize,
    },
    BoolLit {
        value: bool,
        line: usize,
        col: usize,
    },
    Identifier {
        name: std::string::String,
        line: usize,
        col: usize,
    },
    ArrayLit {
        elements: Vec<Expr>,
        line: usize,
        col: usize,
    },
    Grouped(Box<Expr>),
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
        line: usize,
        col: usize,
    },
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expr>,
        line: usize,
        col: usize,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
        line: usize,
        col: usize,
    },
    CompoundAssign {
        target: Box<Expr>,
        op: CompoundOp,
        value: Box<Expr>,
        line: usize,
        col: usize,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        line: usize,
        col: usize,
    },
    MethodCall {
        object: Box<Expr>,
        method: std::string::String,
        args: Vec<Expr>,
        line: usize,
        col: usize,
    },
    FieldAccess {
        object: Box<Expr>,
        field: std::string::String,
        line: usize,
        col: usize,
    },
    IndexAccess {
        object: Box<Expr>,
        index: Box<Expr>,
        line: usize,
        col: usize,
    },
    MacroCall {
        receiver: Option<Box<Expr>>,
        name: std::string::String,
        args: Vec<Expr>,
        line: usize,
        col: usize,
    },
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    Cast {
        value: Box<Expr>,
        target_type: LunaType,
        line: usize,
        col: usize,
    },
}

impl Expr {
    pub fn location(&self) -> (usize, usize) {
        match self {
            Expr::StringLit { line, col, .. }
            | Expr::FStringLit { line, col, .. }
            | Expr::NumberLit { line, col, .. }
            | Expr::BoolLit { line, col, .. }
            | Expr::Identifier { line, col, .. }
            | Expr::ArrayLit { line, col, .. }
            | Expr::BinaryOp { line, col, .. }
            | Expr::UnaryOp { line, col, .. }
            | Expr::Assign { line, col, .. }
            | Expr::CompoundAssign { line, col, .. }
            | Expr::Call { line, col, .. }
            | Expr::MethodCall { line, col, .. }
            | Expr::FieldAccess { line, col, .. }
            | Expr::IndexAccess { line, col, .. }
            | Expr::MacroCall { line, col, .. }
            | Expr::Lambda { line, col, .. }
            | Expr::Cast { line, col, .. } => (*line, *col),
            Expr::Grouped(inner) => inner.location(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Import {
        module: std::string::String,
        alias: Option<std::string::String>,
        line: usize,
        col: usize,
    },
    Use {
        line: usize,
        col: usize,
    },
    VarDecl {
        name: std::string::String,
        mutable: bool,
        ty: LunaType,
        initializer: Option<Expr>,
        is_pub: bool,
        is_static: bool,
        line: usize,
        col: usize,
    },
    ConstDecl {
        name: std::string::String,
        ty: LunaType,
        value: Expr,
        line: usize,
        col: usize,
    },
    FnDecl {
        name: std::string::String,
        params: Vec<Param>,
        return_type: LunaType,
        body: Vec<Stmt>,
        is_async: bool,
        is_pub: bool,
        is_static: bool,
        line: usize,
        col: usize,
    },
    ClassDecl {
        name: std::string::String,
        params: Vec<Param>,
        members: Vec<Stmt>,
        is_pub: bool,
        line: usize,
        col: usize,
    },
    NamespaceDecl {
        name: std::string::String,
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    Return {
        value: Option<Expr>,
        line: usize,
        col: usize,
    },
    Break {
        line: usize,
        col: usize,
    },
    Continue {
        line: usize,
        col: usize,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_if_branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
        line: usize,
        col: usize,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    For {
        variable: std::string::String,
        iterable: Expr,
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    Loop {
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    ExprStmt {
        expr: Expr,
        line: usize,
        col: usize,
    },
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
    pub source_path: std::string::String,
}

impl Program {
    pub fn new(statements: Vec<Stmt>, source_path: impl Into<std::string::String>) -> Self {
        Program {
            statements,
            source_path: source_path.into(),
        }
    }
                    }
    pub fn display(&self) -> std::string::String {
        self.to_string()
    }
}

impl fmt::Display for LunaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LunaType::String      => write!(f, "string"),
            LunaType::Number      => write!(f, "number"),
            LunaType::Bool        => write!(f, "bool"),
            LunaType::Void        => write!(f, "void"),
            LunaType::Null        => write!(f, "null"),
            LunaType::I32         => write!(f, "i32"),
            LunaType::I64         => write!(f, "i64"),
            LunaType::F32         => write!(f, "f32"),
            LunaType::F64         => write!(f, "f64"),
            LunaType::U8          => write!(f, "u8"),
            LunaType::Usize       => write!(f, "usize"),
            LunaType::Custom(n)   => write!(f, "{n}"),
            LunaType::Generic(n, args) => {
                let args_str: Vec<_> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{n}<{}>", args_str.join(", "))
            }
            LunaType::Array(inner)    => write!(f, "[{inner}]"),
            LunaType::Function(ps, r) => {
                let ps_str: Vec<_> = ps.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({}) -> {r}", ps_str.join(", "))
            }
            LunaType::Option(inner) => write!(f, "{inner}?"),
            LunaType::Inferred      => write!(f, "_"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Not,
    Negate,
    BitNot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompoundOp {
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name:    std::string::String,
    pub ty:      LunaType,
    pub default: Option<Expr>,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub enum Expr {
    StringLit  { value: std::string::String, line: usize, col: usize },
    FStringLit { template: std::string::String, line: usize, col: usize },
    NumberLit  { value: f64, line: usize, col: usize },
    BoolLit    { value: bool, line: usize, col: usize },
    NullLit    { line: usize, col: usize },
    Identifier { name: std::string::String, line: usize, col: usize },
    ArrayLit   { elements: Vec<Expr>, line: usize, col: usize },
    Grouped(Box<Expr>),
    BinaryOp {
        left:  Box<Expr>,
        op:    BinaryOperator,
        right: Box<Expr>,
        line:  usize,
        col:   usize,
    },
    UnaryOp {
        op:      UnaryOperator,
        operand: Box<Expr>,
        line:    usize,
        col:     usize,
    },
    Assign {
        target: Box<Expr>,
        value:  Box<Expr>,
        line:   usize,
        col:    usize,
    },
    CompoundAssign {
        target: Box<Expr>,
        op:     CompoundOp,
        value:  Box<Expr>,
        line:   usize,
        col:    usize,
    },
    Call {
        callee: Box<Expr>,
        args:   Vec<Expr>,
        line:   usize,
        col:    usize,
    },
    MethodCall {
        object: Box<Expr>,
        method: std::string::String,
        args:   Vec<Expr>,
        line:   usize,
        col:    usize,
    },
    FieldAccess {
        object: Box<Expr>,
        field:  std::string::String,
        line:   usize,
        col:    usize,
    },
    IndexAccess {
        object: Box<Expr>,
        index:  Box<Expr>,
        line:   usize,
        col:    usize,
    },
    MacroCall {
        receiver: Option<Box<Expr>>,
        name:     std::string::String,
        args:     Vec<Expr>,
        line:     usize,
        col:      usize,
    },
    Lambda {
        params: Vec<Param>,
        body:   Vec<Stmt>,
        line:   usize,
        col:    usize,
    },
    Cast {
        value:       Box<Expr>,
        target_type: LunaType,
        line:        usize,
        col:         usize,
    },
}

impl Expr {
    pub fn location(&self) -> (usize, usize) {
        match self {
            Expr::StringLit  { line, col, .. }
            | Expr::FStringLit { line, col, .. }
            | Expr::NumberLit  { line, col, .. }
            | Expr::BoolLit    { line, col, .. }
            | Expr::NullLit    { line, col }
            | Expr::Identifier { line, col, .. }
            | Expr::ArrayLit   { line, col, .. }
            | Expr::BinaryOp   { line, col, .. }
            | Expr::UnaryOp    { line, col, .. }
            | Expr::Assign     { line, col, .. }
            | Expr::CompoundAssign { line, col, .. }
            | Expr::Call       { line, col, .. }
            | Expr::MethodCall { line, col, .. }
            | Expr::FieldAccess { line, col, .. }
            | Expr::IndexAccess { line, col, .. }
            | Expr::MacroCall  { line, col, .. }
            | Expr::Lambda     { line, col, .. }
            | Expr::Cast       { line, col, .. } => (*line, *col),
            Expr::Grouped(inner) => inner.location(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Import {
        module: std::string::String,
        alias:  Option<std::string::String>,
        line:   usize,
        col:    usize,
    },
    Use {
        line: usize,
        col:  usize,
    },
    VarDecl {
        name:        std::string::String,
        mutable:     bool,
        ty:          LunaType,
        initializer: Option<Expr>,
        is_pub:      bool,
        is_static:   bool,
        line:        usize,
        col:         usize,
    },
    ConstDecl {
        name:  std::string::String,
        ty:    LunaType,
        value: Expr,
        line:  usize,
        col:   usize,
    },
    FnDecl {
        name:        std::string::String,
        params:      Vec<Param>,
        return_type: LunaType,
        body:        Vec<Stmt>,
        is_async:    bool,
        is_pub:      bool,
        is_static:   bool,
        line:        usize,
        col:         usize,
    },
    ClassDecl {
        name:    std::string::String,
        params:  Vec<Param>,
        members: Vec<Stmt>,
        is_pub:  bool,
        line:    usize,
        col:     usize,
    },
    NamespaceDecl {
        name: std::string::String,
        body: Vec<Stmt>,
        line: usize,
        col:  usize,
    },
    Return {
        value: Option<Expr>,
        line:  usize,
        col:   usize,
    },
    Break {
        line: usize,
        col:  usize,
    },
    Continue {
        line: usize,
        col:  usize,
    },
    If {
        condition:        Expr,
        then_body:        Vec<Stmt>,
        else_if_branches: Vec<(Expr, Vec<Stmt>)>,
        else_body:        Option<Vec<Stmt>>,
        line:             usize,
        col:              usize,
    },
    While {
        condition: Expr,
        body:      Vec<Stmt>,
        line:      usize,
        col:       usize,
    },
    For {
        variable: std::string::String,
        iterable: Expr,
        body:     Vec<Stmt>,
        line:     usize,
        col:      usize,
    },
    Loop {
        body: Vec<Stmt>,
        line: usize,
        col:  usize,
    },
    ExprStmt {
        expr: Expr,
        line: usize,
        col:  usize,
    },
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements:  Vec<Stmt>,
    pub source_path: std::string::String,
}

impl Program {
    pub fn new(statements: Vec<Stmt>, source_path: impl Into<std::string::String>) -> Self {
        Program { statements, source_path: source_path.into() }
    }
}
