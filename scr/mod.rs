use std::collections::HashMap;
use crate::ast::nodes::*;
use crate::errors::{LunaError, LunaResult};

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
    Array(Vec<Value>),
    Instance(Box<LunaInstance>),
    Function(Vec<Param>, Vec<Stmt>),
    Void,
    Return(Box<Value>),
    Break,
    Continue,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_)   => "string",
            Value::Number(_)   => "number",
            Value::Bool(_)     => "bool",
            Value::Null        => "null",
            Value::Array(_)    => "array",
            Value::Instance(i) => "instance",
            Value::Function(..)=> "function",
            Value::Void        => "void",
            Value::Return(_)   => "return",
            Value::Break       => "break",
            Value::Continue    => "continue",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b)     => *b,
            Value::Null        => false,
            Value::Number(n)   => *n != 0.0,
            Value::String(s)   => !s.is_empty(),
            Value::Void        => false,
            _                  => true,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s)   => write!(f, "{s}"),
            Value::Number(n)   => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Value::Bool(b)     => write!(f, "{b}"),
            Value::Null        => write!(f, "null"),
            Value::Array(arr)  => {
                let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Instance(i) => write!(f, "<{}>", i.class_name),
            Value::Function(..)=> write!(f, "<function>"),
            Value::Void | Value::Return(_) | Value::Break | Value::Continue => write!(f, ""),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LunaInstance {
    pub class_name: String,
    pub fields:     HashMap<String, Value>,
    pub methods:    HashMap<String, (Vec<Param>, Vec<Stmt>)>,
}

struct Env {
    vars:    HashMap<String, (Value, bool)>,
    fns:     HashMap<String, (Vec<Param>, Vec<Stmt>)>,
    classes: HashMap<String, ClassDef>,
}

#[derive(Clone)]
struct ClassDef {
    params:  Vec<Param>,
    members: Vec<Stmt>,
}

impl Env {
    fn new() -> Self {
        Env {
            vars: HashMap::new(),
            fns:  HashMap::new(),
            classes: HashMap::new(),
        }
    }
}

pub struct Interpreter {
    envs:      Vec<Env>,
    imports:   Vec<String>,
    call_depth: usize,
}

const MAX_CALL_DEPTH: usize = 512;

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Interpreter {
            envs:       Vec::new(),
            imports:    Vec::new(),
            call_depth: 0,
        };
        interp.push_env();
        interp
    }

    fn push_env(&mut self) { self.envs.push(Env::new()); }
    fn pop_env(&mut self)  { self.envs.pop(); }

    fn declare_var(&mut self, name: &str, value: Value, mutable: bool) {
        if let Some(env) = self.envs.last_mut() {
            env.vars.insert(name.to_string(), (value, mutable));
        }
    }

    fn get_var(&self, name: &str) -> Option<Value> {
        self.envs.iter().rev().find_map(|e| e.vars.get(name).map(|(v, _)| v.clone()))
    }

    fn set_var(&mut self, name: &str, value: Value) -> LunaResult<()> {
        for env in self.envs.iter_mut().rev() {
            if let Some((existing, mutable)) = env.vars.get_mut(name) {
                if !*mutable {
                    return Err(LunaError::ImmutableAssignment {
                        name: name.to_string(), line: 0, col: 0,
                    });
                }
                *existing = value;
                return Ok(());
            }
        }
        if let Some(env) = self.envs.last_mut() {
            env.vars.insert(name.to_string(), (value, true));
        }
        Ok(())
    }

    fn declare_fn(&mut self, name: &str, params: Vec<Param>, body: Vec<Stmt>) {
        if let Some(env) = self.envs.last_mut() {
            env.fns.insert(name.to_string(), (params, body));
        }
    }

    fn get_fn(&self, name: &str) -> Option<(Vec<Param>, Vec<Stmt>)> {
        self.envs.iter().rev().find_map(|e| e.fns.get(name).cloned())
    }

    fn declare_class(&mut self, name: &str, def: ClassDef) {
        if let Some(env) = self.envs.last_mut() {
            env.classes.insert(name.to_string(), def);
        }
    }

    fn get_class(&self, name: &str) -> Option<ClassDef> {
        self.envs.iter().rev().find_map(|e| e.classes.get(name).cloned())
    }

    fn interpolate_fstring(&self, template: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = template.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                i += 2;
                let mut var = String::new();
                while i < chars.len() && chars[i] != '}' {
                    var.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() { i += 1; }
                let val = self.get_var(var.trim()).unwrap_or(Value::Null);
                result.push_str(&val.to_string());
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    pub fn eval(&mut self, expr: &Expr) -> LunaResult<Value> {
        match expr {
            Expr::StringLit  { value, .. }   => Ok(Value::String(value.clone())),
            Expr::FStringLit { template, .. } => Ok(Value::String(self.interpolate_fstring(template))),
            Expr::NumberLit  { value, .. }   => Ok(Value::Number(*value)),
            Expr::BoolLit    { value, .. }   => Ok(Value::Bool(*value)),
            Expr::NullLit    { .. }          => Ok(Value::Null),

            Expr::ArrayLit { elements, .. } => {
                let mut arr = Vec::new();
                for e in elements { arr.push(self.eval(e)?); }
                Ok(Value::Array(arr))
            }

            Expr::Grouped(inner) => self.eval(inner),

            Expr::Identifier { name, line, col } => {
                self.get_var(name).ok_or_else(|| LunaError::UndeclaredVariable {
                    name: name.clone(), line: *line, col: *col,
                })
            }

            Expr::UnaryOp { op, operand, line, col } => {
                let val = self.eval(operand)?;
                match op {
                    UnaryOperator::Not    => Ok(Value::Bool(!val.is_truthy())),
                    UnaryOperator::Negate => match val {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        _ => Err(LunaError::RuntimeError {
                            message: format!("cannot negate a {}", val.type_name()),
                        }),
                    },
                    UnaryOperator::BitNot => match val {
                        Value::Number(n) => Ok(Value::Number(!((n as i64)) as f64)),
                        _ => Err(LunaError::RuntimeError {
                            message: "bitwise NOT requires a number".to_string(),
                        }),
                    },
                }
            }

            Expr::BinaryOp { left, op, right, line, col } => {
                if matches!(op, BinaryOperator::And) {
                    let lv = self.eval(left)?;
                    if !lv.is_truthy() { return Ok(Value::Bool(false)); }
                    let rv = self.eval(right)?;
                    return Ok(Value::Bool(rv.is_truthy()));
                }
                if matches!(op, BinaryOperator::Or) {
                    let lv = self.eval(left)?;
                    if lv.is_truthy() { return Ok(Value::Bool(true)); }
                    let rv = self.eval(right)?;
                    return Ok(Value::Bool(rv.is_truthy()));
                }

                let lv = self.eval(left)?;
                let rv = self.eval(right)?;
                self.apply_binary(op, lv, rv, *line, *col)
            }

            Expr::Assign { target, value, line, col } => {
                let val = self.eval(value)?;
                match target.as_ref() {
                    Expr::Identifier { name, .. } => self.set_var(name, val.clone())?,
                    Expr::FieldAccess { object, field, .. } => {
                        let obj = self.eval(object)?;
                        if let Value::Instance(mut inst) = obj {
                            inst.fields.insert(field.clone(), val.clone());
                        }
                    }
                    Expr::IndexAccess { object, index, .. } => {
                        let _idx = self.eval(index)?;
                    }
                    _ => {}
                }
                Ok(val)
            }

            Expr::CompoundAssign { target, op, value, line, col } => {
                let current = if let Expr::Identifier { name, .. } = target.as_ref() {
                    self.get_var(name).unwrap_or(Value::Number(0.0))
                } else { Value::Number(0.0) };
                let rval = self.eval(value)?;
                let bin_op = match op {
                    CompoundOp::AddAssign => BinaryOperator::Add,
                    CompoundOp::SubAssign => BinaryOperator::Subtract,
                    CompoundOp::MulAssign => BinaryOperator::Multiply,
                    CompoundOp::DivAssign => BinaryOperator::Divide,
                };
                let result = self.apply_binary(&bin_op, current, rval, *line, *col)?;
                if let Expr::Identifier { name, .. } = target.as_ref() {
                    self.set_var(name, result.clone())?;
                }
                Ok(result)
            }

            Expr::MacroCall { name, args, receiver, .. } => {
                self.call_builtin_macro(name, args, receiver.as_deref())
            }

            Expr::Call { callee, args, line, col } => {
                match callee.as_ref() {
                    Expr::Identifier { name, .. } => {
                        if let Some(class_def) = self.get_class(name) {
                            return self.instantiate(name, &class_def, args);
                        }
                        if let Some((params, body)) = self.get_fn(name) {
                            return self.call_fn(name, &params, &body, args);
                        }
                        self.call_builtin(name, args, *line, *col)
                    }
                    _ => {
                        let fn_val = self.eval(callee)?;
                        if let Value::Function(params, body) = fn_val {
                            let params_clone = params.clone();
                            let body_clone = body.clone();
                            self.call_fn("<closure>", &params_clone, &body_clone, args)
                        } else {
                            Err(LunaError::RuntimeError {
                                message: "called value is not a function".to_string(),
                            })
                        }
                    }
                }
            }

            Expr::MethodCall { object, method, args, line, col } => {
                let obj_val = self.eval(object)?;
                self.call_method(obj_val, method, args, *line, *col)
            }

            Expr::FieldAccess { object, field, line, col } => {
                let obj_val = self.eval(object)?;
                match obj_val {
                    Value::Instance(inst) => {
                        inst.fields.get(field).cloned().ok_or_else(|| LunaError::UndefinedProperty {
                            class_name: inst.class_name.clone(),
                            property: field.clone(),
                        })
                    }
                    Value::Array(arr) if field == "length" || field == "len" => {
                        Ok(Value::Number(arr.len() as f64))
                    }
                    Value::String(s) if field == "length" || field == "len" => {
                        Ok(Value::Number(s.chars().count() as f64))
                    }
                    _ => Err(LunaError::RuntimeError {
                        message: format!("cannot access field `{field}` on {}", obj_val.type_name()),
                    }),
                }
            }

            Expr::IndexAccess { object, index, line, col } => {
                let obj_val = self.eval(object)?;
                let idx_val = self.eval(index)?;
                match (obj_val, idx_val) {
                    (Value::Array(arr), Value::Number(n)) => {
                        let i = n as usize;
                        arr.get(i).cloned().ok_or_else(|| LunaError::RuntimeError {
                            message: format!("index {i} out of bounds (length {})", arr.len()),
                        })
                    }
                    (Value::String(s), Value::Number(n)) => {
                        let i = n as usize;
                        let ch = s.chars().nth(i).ok_or_else(|| LunaError::RuntimeError {
                            message: format!("index {i} out of bounds"),
                        })?;
                        Ok(Value::String(ch.to_string()))
                    }
                    _ => Err(LunaError::RuntimeError {
                        message: "invalid index operation".to_string(),
                    }),
                }
            }

            Expr::Lambda { params, body, .. } => {
                Ok(Value::Function(params.clone(), body.clone()))
            }

            Expr::Cast { value, target_type, .. } => {
                let val = self.eval(value)?;
                match (val, target_type) {
                    (Value::Number(n), LunaType::String) => Ok(Value::String(format!("{}", n as i64))),
                    (Value::String(s), LunaType::Number) => {
                        Ok(Value::Number(s.parse::<f64>().unwrap_or(0.0)))
                    }
                    (Value::Bool(b), LunaType::Number) => Ok(Value::Number(if b { 1.0 } else { 0.0 })),
                    (v, _) => Ok(v),
                }
            }
        }
    }

    fn apply_binary(&self, op: &BinaryOperator, lv: Value, rv: Value, line: usize, col: usize) -> LunaResult<Value> {
        match op {
            BinaryOperator::Add => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
                (Value::String(a), b)                => Ok(Value::String(a + &b.to_string())),
                (a, Value::String(b))                => Ok(Value::String(a.to_string() + &b)),
                (Value::Array(mut a), Value::Array(b)) => { a.extend(b); Ok(Value::Array(a)) }
                (a, b) => Err(LunaError::InvalidOperandTypes {
                    op: "+".into(), left: a.type_name().into(), right: b.type_name().into(), line, col,
                }),
            },
            BinaryOperator::Subtract => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
                (a, b) => Err(LunaError::InvalidOperandTypes { op: "-".into(), left: a.type_name().into(), right: b.type_name().into(), line, col }),
            },
            BinaryOperator::Multiply => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
                (Value::String(s), Value::Number(n)) => Ok(Value::String(s.repeat(n as usize))),
                (a, b) => Err(LunaError::InvalidOperandTypes { op: "*".into(), left: a.type_name().into(), right: b.type_name().into(), line, col }),
            },
            BinaryOperator::Divide => match (lv, rv) {
                (Value::Number(_), Value::Number(b)) if b == 0.0 => Err(LunaError::DivisionByZero { line, col }),
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
                (a, b) => Err(LunaError::InvalidOperandTypes { op: "/".into(), left: a.type_name().into(), right: b.type_name().into(), line, col }),
            },
            BinaryOperator::Modulo => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
                (a, b) => Err(LunaError::InvalidOperandTypes { op: "%".into(), left: a.type_name().into(), right: b.type_name().into(), line, col }),
            },
            BinaryOperator::Power => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(b))),
                (a, b) => Err(LunaError::InvalidOperandTypes { op: "**".into(), left: a.type_name().into(), right: b.type_name().into(), line, col }),
            },
            BinaryOperator::Equal        => Ok(Value::Bool(self.values_eq(&lv, &rv))),
            BinaryOperator::NotEqual     => Ok(Value::Bool(!self.values_eq(&lv, &rv))),
            BinaryOperator::Less         => Ok(Value::Bool(self.value_cmp(&lv, &rv) < 0)),
            BinaryOperator::LessEqual    => Ok(Value::Bool(self.value_cmp(&lv, &rv) <= 0)),
            BinaryOperator::Greater      => Ok(Value::Bool(self.value_cmp(&lv, &rv) > 0)),
            BinaryOperator::GreaterEqual => Ok(Value::Bool(self.value_cmp(&lv, &rv) >= 0)),
            BinaryOperator::And          => Ok(Value::Bool(lv.is_truthy() && rv.is_truthy())),
            BinaryOperator::Or           => Ok(Value::Bool(lv.is_truthy() || rv.is_truthy())),
            BinaryOperator::BitAnd       => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(((a as i64) & (b as i64)) as f64)),
                _ => Ok(Value::Null),
            },
            BinaryOperator::BitOr        => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(((a as i64) | (b as i64)) as f64)),
                _ => Ok(Value::Null),
            },
            BinaryOperator::BitXor       => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(((a as i64) ^ (b as i64)) as f64)),
                _ => Ok(Value::Null),
            },
            BinaryOperator::ShiftLeft    => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(((a as i64) << (b as u32)) as f64)),
                _ => Ok(Value::Null),
            },
            BinaryOperator::ShiftRight   => match (lv, rv) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(((a as i64) >> (b as u32)) as f64)),
                _ => Ok(Value::Null),
            },
        }
    }

    fn values_eq(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x == y,
            (Value::String(x), Value::String(y)) => x == y,
            (Value::Bool(x), Value::Bool(y))     => x == y,
            (Value::Null, Value::Null)            => true,
            (Value::Void, Value::Void)            => true,
            _ => false,
        }
    }

    fn value_cmp(&self, a: &Value, b: &Value) -> i32 {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).map(|o| o as i32).unwrap_or(0),
            (Value::String(x), Value::String(y)) => x.cmp(y) as i32,
            _ => 0,
        }
    }

    fn call_fn(&mut self, name: &str, params: &[Param], body: &[Stmt], args: &[Expr]) -> LunaResult<Value> {
        self.call_depth += 1;
        if self.call_depth > MAX_CALL_DEPTH {
            self.call_depth -= 1;
            return Err(LunaError::StackOverflow { fn_name: name.to_string() });
        }

        self.push_env();

        let mut arg_vals = Vec::new();
        for arg in args { arg_vals.push(self.eval(arg)?); }

        for (i, param) in params.iter().enumerate() {
            let val = arg_vals.get(i).cloned().unwrap_or(Value::Null);
            self.declare_var(&param.name, val, param.mutable);
        }

        let mut result = Value::Void;
        for stmt in body {
            let r = self.exec(stmt)?;
            match r {
                Value::Return(v) => { result = *v; break; }
                Value::Break | Value::Continue => { result = r; break; }
                _ => {}
            }
        }

        self.pop_env();
        self.call_depth -= 1;
        Ok(result)
    }

    fn instantiate(&mut self, class_name: &str, def: &ClassDef, args: &[Expr]) -> LunaResult<Value> {
        let mut fields  = HashMap::new();
        let mut methods = HashMap::new();
        let members = def.members.clone();

        for member in &members {
            match member {
                Stmt::VarDecl { name, initializer, .. } => {
                    let val = if let Some(init) = initializer {
                        self.eval(init)?
                    } else { Value::Null };
                    fields.insert(name.clone(), val);
                }
                Stmt::ConstDecl { name, value, .. } => {
                    let val = self.eval(value)?;
                    fields.insert(name.clone(), val);
                }
                Stmt::FnDecl { name, params, body, .. } => {
                    methods.insert(name.clone(), (params.clone(), body.clone()));
                }
                _ => {}
            }
        }

        let mut arg_vals = Vec::new();
        for arg in args { arg_vals.push(self.eval(arg)?); }
        for (i, param) in def.params.iter().enumerate() {
            if let Some(val) = arg_vals.get(i).cloned() {
                fields.insert(param.name.clone(), val);
            }
        }

        Ok(Value::Instance(Box::new(LunaInstance {
            class_name: class_name.to_string(),
            fields,
            methods,
        })))
    }

    fn call_method(&mut self, obj: Value, method: &str, args: &[Expr], line: usize, col: usize) -> LunaResult<Value> {
        match obj {
            Value::Instance(mut inst) => {
                let (params, body) = inst.methods.get(method).cloned().ok_or_else(|| {
                    LunaError::UndefinedMethod {
                        class_name: inst.class_name.clone(),
                        method: method.to_string(),
                    }
                })?;

                self.push_env();
                for (name, val) in &inst.fields {
                    self.declare_var(name, val.clone(), true);
                }

                let mut arg_vals = Vec::new();
                for arg in args { arg_vals.push(self.eval(arg)?); }
                for (i, param) in params.iter().enumerate() {
                    let val = arg_vals.get(i).cloned().unwrap_or(Value::Null);
                    self.declare_var(&param.name, val, param.mutable);
                }

                let mut result = Value::Void;
                for stmt in &body {
                    let r = self.exec(stmt)?;
                    match r {
                        Value::Return(v) => { result = *v; break; }
                        _ => {}
                    }
                }

                for name in inst.fields.keys().cloned().collect::<Vec<_>>() {
                    if let Some(new_val) = self.get_var(&name) {
                        inst.fields.insert(name, new_val);
                    }
                }
                self.pop_env();
                Ok(result)
            }

            Value::String(s) => self.string_method(&s, method, args, line, col),
            Value::Array(arr) => self.array_method(arr, method, args, line, col),

            _ => Err(LunaError::RuntimeError {
                message: format!("cannot call method `{method}` on {}", obj.type_name()),
            }),
        }
    }

    fn call_builtin_macro(&mut self, name: &str, args: &[Expr], receiver: Option<&Expr>) -> LunaResult<Value> {
        match name {
            "println" => {
                let parts: Vec<String> = args.iter()
                    .map(|a| self.eval(a).map(|v| v.to_string()))
                    .collect::<LunaResult<Vec<_>>>()?;
                println!("{}", parts.join(""));
                Ok(Value::Void)
            }
            "print" => {
                let parts: Vec<String> = args.iter()
                    .map(|a| self.eval(a).map(|v| v.to_string()))
                    .collect::<LunaResult<Vec<_>>>()?;
                print!("{}", parts.join(""));
                Ok(Value::Void)
            }
            "eprintln" => {
                let parts: Vec<String> = args.iter()
                    .map(|a| self.eval(a).map(|v| v.to_string()))
                    .collect::<LunaResult<Vec<_>>>()?;
                eprintln!("{}", parts.join(""));
                Ok(Value::Void)
            }
            "assert" => {
                if args.is_empty() { return Ok(Value::Void); }
                let val = self.eval(&args[0])?;
                if !val.is_truthy() {
                    let msg = if args.len() > 1 {
                        self.eval(&args[1])?.to_string()
                    } else {
                        "assertion failed".to_string()
                    };
                    return Err(LunaError::RuntimeError { message: msg });
                }
                Ok(Value::Void)
            }
            "dbg" => {
                for arg in args {
                    let val = self.eval(arg)?;
                    eprintln!("[dbg] {:?}", val);
                }
                Ok(Value::Void)
            }
            _ => Ok(Value::Void),
        }
    }

    fn call_builtin(&mut self, name: &str, args: &[Expr], line: usize, col: usize) -> LunaResult<Value> {
        let mut vals = Vec::new();
        for a in args { vals.push(self.eval(a)?); }

        match name {
            "input" => {
                if let Some(Value::String(prompt)) = vals.first() {
                    print!("{prompt}");
                }
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                Ok(Value::String(line.trim_end_matches('\n').to_string()))
            }

            "string"  => Ok(Value::String(vals.first().map(|v| v.to_string()).unwrap_or_default())),
            "number"  => match vals.first() {
                Some(Value::String(s)) => Ok(Value::Number(s.parse::<f64>().unwrap_or(0.0))),
                Some(Value::Number(n)) => Ok(Value::Number(*n)),
                Some(Value::Bool(b))   => Ok(Value::Number(if *b { 1.0 } else { 0.0 })),
                _                      => Ok(Value::Number(0.0)),
            },
            "bool" => match vals.first() {
                Some(v) => Ok(Value::Bool(v.is_truthy())),
                None    => Ok(Value::Bool(false)),
            },
            "int" | "floor" => match vals.first() {
                Some(Value::Number(n)) => Ok(Value::Number(n.floor())),
                _ => Ok(Value::Number(0.0)),
            },

            "sqrt"  => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.sqrt())), _ => Ok(Value::Number(0.0)) },
            "abs"   => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.abs())),  _ => Ok(Value::Number(0.0)) },
            "ceil"  => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.ceil())), _ => Ok(Value::Number(0.0)) },
            "round" => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.round())),_ => Ok(Value::Number(0.0)) },
            "min"   => match (vals.first(), vals.get(1)) {
                (Some(Value::Number(a)), Some(Value::Number(b))) => Ok(Value::Number(a.min(*b))),
                _ => Ok(Value::Number(0.0)),
            },
            "max"   => match (vals.first(), vals.get(1)) {
                (Some(Value::Number(a)), Some(Value::Number(b))) => Ok(Value::Number(a.max(*b))),
                _ => Ok(Value::Number(0.0)),
            },
            "pow"   => match (vals.first(), vals.get(1)) {
                (Some(Value::Number(a)), Some(Value::Number(b))) => Ok(Value::Number(a.powf(*b))),
                _ => Ok(Value::Number(0.0)),
            },
            "log"   => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.ln())), _ => Ok(Value::Number(0.0)) },
            "log2"  => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.log2())), _ => Ok(Value::Number(0.0)) },
            "sin"   => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.sin())), _ => Ok(Value::Number(0.0)) },
            "cos"   => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.cos())), _ => Ok(Value::Number(0.0)) },
            "tan"   => match vals.first() { Some(Value::Number(n)) => Ok(Value::Number(n.tan())), _ => Ok(Value::Number(0.0)) },

            "typeof" => Ok(Value::String(vals.first().map(|v| v.type_name().to_string()).unwrap_or_default())),
            "len"    => match vals.first() {
                Some(Value::String(s)) => Ok(Value::Number(s.chars().count() as f64)),
                Some(Value::Array(a))  => Ok(Value::Number(a.len() as f64)),
                _ => Ok(Value::Number(0.0)),
            },
            "range"  => match (vals.first(), vals.get(1)) {
                (Some(Value::Number(start)), Some(Value::Number(end))) => {
                    let arr: Vec<Value> = (*start as i64..*end as i64).map(|n| Value::Number(n as f64)).collect();
                    Ok(Value::Array(arr))
                }
                (Some(Value::Number(end)), None) => {
                    let arr: Vec<Value> = (0..*end as i64).map(|n| Value::Number(n as f64)).collect();
                    Ok(Value::Array(arr))
                }
                _ => Ok(Value::Array(vec![])),
            },

            _ => Err(LunaError::UndeclaredFunction { name: name.to_string(), line, col }),
        }
    }

    fn string_method(&mut self, s: &str, method: &str, args: &[Expr], line: usize, col: usize) -> LunaResult<Value> {
        let mut arg_vals = Vec::new();
        for a in args { arg_vals.push(self.eval(a)?); }

        match method {
            "length" | "len"  => Ok(Value::Number(s.chars().count() as f64)),
            "upper"  | "toUpperCase" => Ok(Value::String(s.to_uppercase())),
            "lower"  | "toLowerCase" => Ok(Value::String(s.to_lowercase())),
            "trim"                    => Ok(Value::String(s.trim().to_string())),
            "trimStart"               => Ok(Value::String(s.trim_start().to_string())),
            "trimEnd"                 => Ok(Value::String(s.trim_end().to_string())),
            "reverse"                 => Ok(Value::String(s.chars().rev().collect())),
            "contains" => match arg_vals.first() {
                Some(Value::String(sub)) => Ok(Value::Bool(s.contains(sub.as_str()))),
                _ => Ok(Value::Bool(false)),
            },
            "startsWith" => match arg_vals.first() {
                Some(Value::String(sub)) => Ok(Value::Bool(s.starts_with(sub.as_str()))),
                _ => Ok(Value::Bool(false)),
            },
            "endsWith" => match arg_vals.first() {
                Some(Value::String(sub)) => Ok(Value::Bool(s.ends_with(sub.as_str()))),
                _ => Ok(Value::Bool(false)),
            },
            "indexOf" => match arg_vals.first() {
                Some(Value::String(sub)) => Ok(Value::Number(
                    s.find(sub.as_str()).map(|i| i as f64).unwrap_or(-1.0)
                )),
                _ => Ok(Value::Number(-1.0)),
            },
            "replace" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(Value::String(from)), Some(Value::String(to))) => {
                    Ok(Value::String(s.replace(from.as_str(), to.as_str())))
                }
                _ => Ok(Value::String(s.to_string())),
            },
            "split" => match arg_vals.first() {
                Some(Value::String(sep)) => {
                    let parts: Vec<Value> = s.split(sep.as_str())
                        .map(|p| Value::String(p.to_string()))
                        .collect();
                    Ok(Value::Array(parts))
                }
                _ => Ok(Value::Array(vec![Value::String(s.to_string())])),
            },
            "slice" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(Value::Number(start)), Some(Value::Number(end))) => {
                    let chars: Vec<char> = s.chars().collect();
                    let s = *start as usize;
                    let e = (*end as usize).min(chars.len());
                    Ok(Value::String(chars[s..e].iter().collect()))
                }
                _ => Ok(Value::String(s.to_string())),
            },
            "repeat" => match arg_vals.first() {
                Some(Value::Number(n)) => Ok(Value::String(s.repeat(*n as usize))),
                _ => Ok(Value::String(s.to_string())),
            },
            "chars" => {
                let arr: Vec<Value> = s.chars().map(|c| Value::String(c.to_string())).collect();
                Ok(Value::Array(arr))
            }
            _ => Err(LunaError::UndefinedMethod {
                class_name: "string".to_string(),
                method: method.to_string(),
            }),
        }
    }

    fn array_method(&mut self, mut arr: Vec<Value>, method: &str, args: &[Expr], line: usize, col: usize) -> LunaResult<Value> {
        let mut arg_vals = Vec::new();
        for a in args { arg_vals.push(self.eval(a)?); }

        match method {
            "length" | "len" => Ok(Value::Number(arr.len() as f64)),
            "push" | "add"   => {
                for v in arg_vals { arr.push(v); }
                Ok(Value::Array(arr))
            }
            "pop"    => { let v = arr.pop().unwrap_or(Value::Null); Ok(v) }
            "first"  => Ok(arr.first().cloned().unwrap_or(Value::Null)),
            "last"   => Ok(arr.last().cloned().unwrap_or(Value::Null)),
            "reverse" => { arr.reverse(); Ok(Value::Array(arr)) }
            "join" => {
                let sep = match arg_vals.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => ", ".to_string(),
                };
                let parts: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                Ok(Value::String(parts.join(&sep)))
            }
            "contains" => {
                if let Some(target) = arg_vals.first() {
                    let found = arr.iter().any(|v| self.values_eq(v, target));
                    Ok(Value::Bool(found))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            "slice" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(Value::Number(s)), Some(Value::Number(e))) => {
                    let start = *s as usize;
                    let end = (*e as usize).min(arr.len());
                    Ok(Value::Array(arr[start..end].to_vec()))
                }
                _ => Ok(Value::Array(arr)),
            },
            _ => Err(LunaError::UndefinedMethod {
                class_name: "array".to_string(),
                method: method.to_string(),
            }),
        }
    }

    pub fn exec(&mut self, stmt: &Stmt) -> LunaResult<Value> {
        match stmt {
            Stmt::Import { module, .. } => {
                self.imports.push(module.clone());
                Ok(Value::Void)
            }
            Stmt::Use { .. } => Ok(Value::Void),

            Stmt::VarDecl { name, mutable, initializer, .. } => {
                let val = if let Some(init) = initializer {
                    self.eval(init)?
                } else { Value::Null };
                self.declare_var(name, val, *mutable);
                Ok(Value::Void)
            }

            Stmt::ConstDecl { name, value, .. } => {
                let val = self.eval(value)?;
                self.declare_var(name, val, false);
                Ok(Value::Void)
            }

            Stmt::FnDecl { name, params, body, .. } => {
                self.declare_fn(name, params.clone(), body.clone());
                Ok(Value::Void)
            }

            Stmt::ClassDecl { name, params, members, .. } => {
                self.declare_class(name, ClassDef { params: params.clone(), members: members.clone() });
                Ok(Value::Void)
            }

            Stmt::NamespaceDecl { body, .. } => {
                self.push_env();
                for s in body { self.exec(s)?; }
                self.pop_env();
                Ok(Value::Void)
            }

            Stmt::Return { value, .. } => {
                let val = if let Some(expr) = value { self.eval(expr)? } else { Value::Void };
                Ok(Value::Return(Box::new(val)))
            }
            Stmt::Break    { .. } => Ok(Value::Break),
            Stmt::Continue { .. } => Ok(Value::Continue),

            Stmt::If { condition, then_body, else_if_branches, else_body, .. } => {
                let cond = self.eval(condition)?;
                let branch = if cond.is_truthy() {
                    Some(then_body.as_slice())
                } else {
                    let mut chosen = None;
                    for (elif_cond, elif_body) in else_if_branches {
                        if self.eval(elif_cond)?.is_truthy() {
                            chosen = Some(elif_body.as_slice());
                            break;
                        }
                    }
                    chosen.or_else(|| else_body.as_deref())
                };

                if let Some(stmts) = branch {
                    self.push_env();
                    let mut result = Value::Void;
                    for s in stmts {
                        result = self.exec(s)?;
                        if matches!(result, Value::Return(_) | Value::Break | Value::Continue) { break; }
                    }
                    self.pop_env();
                    Ok(result)
                } else {
                    Ok(Value::Void)
                }
            }

            Stmt::While { condition, body, .. } => {
                loop {
                    let cond = self.eval(condition)?;
                    if !cond.is_truthy() { break; }
                    self.push_env();
                    let mut ret = Value::Void;
                    for s in body {
                        ret = self.exec(s)?;
                        match &ret {
                            Value::Break    => { self.pop_env(); return Ok(Value::Void); }
                            Value::Continue => break,
                            Value::Return(_) => { self.pop_env(); return Ok(ret); }
                            _ => {}
                        }
                    }
                    self.pop_env();
                }
                Ok(Value::Void)
            }

            Stmt::For { variable, iterable, body, line, col } => {
                let iter_val = self.eval(iterable)?;
                let items = match iter_val {
                    Value::Array(arr) => arr,
                    Value::String(s)  => s.chars().map(|c| Value::String(c.to_string())).collect(),
                    _ => return Err(LunaError::RuntimeError {
                        message: format!("cannot iterate over {}", iter_val.type_name()),
                    }),
                };
                for item in items {
                    self.push_env();
                    self.declare_var(variable, item, false);
                    let mut ret = Value::Void;
                    for s in body {
                        ret = self.exec(s)?;
                        match &ret {
                            Value::Break    => { self.pop_env(); return Ok(Value::Void); }
                            Value::Continue => break,
                            Value::Return(_) => { self.pop_env(); return Ok(ret); }
                            _ => {}
                        }
                    }
                    self.pop_env();
                }
                Ok(Value::Void)
            }

            Stmt::Loop { body, .. } => {
                loop {
                    self.push_env();
                    let mut ret = Value::Void;
                    for s in body {
                        ret = self.exec(s)?;
                        match &ret {
                            Value::Break    => { self.pop_env(); return Ok(Value::Void); }
                            Value::Continue => break,
                            Value::Return(_) => { self.pop_env(); return Ok(ret); }
                            _ => {}
                        }
                    }
                    self.pop_env();
                }
            }

            Stmt::ExprStmt { expr, .. } => self.eval(expr),
        }
    }

    pub fn run(&mut self, program: &Program) -> LunaResult<()> {
        for stmt in &program.statements {
            match stmt {
                Stmt::Import { .. } | Stmt::FnDecl { .. } | Stmt::ClassDecl { .. } => {
                    self.exec(stmt)?;
                }
                _ => {}
            }
        }

        if let Some(main_class) = self.get_class("main") {
            let fn_entry = main_class.members.iter().find_map(|m| {
                if let Stmt::FnDecl { name, params, body, .. } = m {
                    if name == "main" { Some((params.clone(), body.clone())) }
                    else { None }
                } else { None }
            });
            if let Some((params, body)) = fn_entry {
                self.call_fn("main", &params, &body, &[])?;
                return Ok(());
            }
        }

        if let Some((params, body)) = self.get_fn("main") {
            self.call_fn("main", &params, &body, &[])?;
            return Ok(());
        }

        for stmt in &program.statements.clone() {
            match stmt {
                Stmt::Import { .. } | Stmt::FnDecl { .. } | Stmt::ClassDecl { .. } => {}
                other => { self.exec(other)?; }
            }
        }

        Ok(())
    }
  }
