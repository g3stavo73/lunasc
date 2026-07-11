use std::collections::HashMap;
use crate::ast::nodes::*;
use crate::errors::{LunaError, LunaResult};

#[derive(Debug, Clone)]
struct VarInfo {
    pub ty:      LunaType,
    pub mutable: bool,
    pub line:    usize,
    pub col:     usize,
}

#[derive(Debug, Clone)]
struct FnInfo {
    pub params:      Vec<LunaType>,
    pub return_type: LunaType,
}

#[derive(Debug, Clone)]
struct ClassInfo {
    pub fields:  Vec<(String, LunaType)>,
    pub methods: HashMap<String, FnInfo>,
}

#[derive(Debug)]
struct Scope {
    vars:    HashMap<String, VarInfo>,
    fns:     HashMap<String, FnInfo>,
    classes: HashMap<String, ClassInfo>,
}

impl Scope {
    fn new() -> Self {
        Scope {
            vars: HashMap::new(),
            fns: HashMap::new(),
            classes: HashMap::new(),
        }
    }
}

pub struct SemanticChecker {
    scopes:   Vec<Scope>,
    imports:  Vec<String>,
    fn_stack: Vec<LunaType>,
}

impl SemanticChecker {
    pub fn new() -> Self {
        let mut checker = SemanticChecker {
            scopes:   Vec::new(),
            imports:  Vec::new(),
            fn_stack: Vec::new(),
        };
        checker.push_scope();
        checker.register_builtins();
        checker
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn current_scope_mut(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("no scope")
    }

    fn declare_var(&mut self, name: &str, ty: LunaType, mutable: bool, line: usize, col: usize) -> LunaResult<()> {
        if self.current_scope_mut().vars.contains_key(name) {
            return Err(LunaError::DuplicateDeclaration { name: name.to_string(), line, col });
        }
        self.current_scope_mut().vars.insert(name.to_string(), VarInfo { ty, mutable, line, col });
        Ok(())
    }

    fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        self.scopes.iter().rev().find_map(|s| s.vars.get(name))
    }

    fn lookup_var_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        self.scopes.iter_mut().rev().find_map(|s| s.vars.get_mut(name))
    }

    fn declare_fn(&mut self, name: &str, params: Vec<LunaType>, return_type: LunaType) {
        self.current_scope_mut().fns.insert(name.to_string(), FnInfo { params, return_type });
    }

    fn lookup_fn(&self, name: &str) -> Option<&FnInfo> {
        self.scopes.iter().rev().find_map(|s| s.fns.get(name))
    }

    fn declare_class(&mut self, name: &str, info: ClassInfo) {
        self.current_scope_mut().classes.insert(name.to_string(), info);
    }

    fn lookup_class(&self, name: &str) -> Option<&ClassInfo> {
        self.scopes.iter().rev().find_map(|s| s.classes.get(name))
    }

    fn register_builtins(&mut self) {
        self.declare_fn("assert", vec![LunaType::Bool], LunaType::Void);
        self.declare_fn("typeof", vec![LunaType::Inferred], LunaType::String);
    }

    fn types_compatible(a: &LunaType, b: &LunaType) -> bool {
        if a == b { return true; }
        if matches!(a, LunaType::Inferred) || matches!(b, LunaType::Inferred) { return true; }
        if a.is_numeric() && b.is_numeric() { return true; }
        if matches!(b, LunaType::Null) && matches!(a, LunaType::Option(_)) { return true; }
        if matches!(a, LunaType::Null) && matches!(b, LunaType::Option(_)) { return true; }
        false
    }

    pub fn infer_expr(&mut self, expr: &Expr) -> LunaResult<LunaType> {
        match expr {
            Expr::StringLit  { .. } | Expr::FStringLit { .. } => Ok(LunaType::String),
            Expr::NumberLit  { .. }                            => Ok(LunaType::Number),
            Expr::BoolLit    { .. }                            => Ok(LunaType::Bool),
            Expr::NullLit    { .. }                            => Ok(LunaType::Null),
            Expr::ArrayLit   { elements, .. } => {
                if elements.is_empty() {
                    return Ok(LunaType::Array(Box::new(LunaType::Inferred)));
                }
                let elem_ty = self.infer_expr(&elements[0])?;
                for e in elements.iter().skip(1) {
                    let t = self.infer_expr(e)?;
                    if !Self::types_compatible(&elem_ty, &t) {
                        let (line, col) = e.location();
                        return Err(LunaError::TypeMismatch {
                            expected: elem_ty.display(),
                            found: t.display(),
                            line, col,
                        });
                    }
                }
                Ok(LunaType::Array(Box::new(elem_ty)))
            }

            Expr::Identifier { name, line, col } => {
                if let Some(info) = self.lookup_var(name) {
                    Ok(info.ty.clone())
                } else {
                    Err(LunaError::UndeclaredVariable { name: name.clone(), line: *line, col: *col })
                }
            }

            Expr::Grouped(inner) => self.infer_expr(inner),

            Expr::BinaryOp { left, op, right, line, col } => {
                let lt = self.infer_expr(left)?;
                let rt = self.infer_expr(right)?;
                match op {
                    BinaryOperator::Add => {
                        if lt == LunaType::String || rt == LunaType::String {
                            Ok(LunaType::String)
                        } else if lt.is_numeric() && rt.is_numeric() {
                            Ok(LunaType::Number)
                        } else {
                            Err(LunaError::InvalidOperandTypes {
                                op: "+".to_string(),
                                left: lt.display(), right: rt.display(),
                                line: *line, col: *col,
                            })
                        }
                    }
                    BinaryOperator::Subtract | BinaryOperator::Multiply
                    | BinaryOperator::Divide | BinaryOperator::Modulo
                    | BinaryOperator::Power   | BinaryOperator::ShiftLeft
                    | BinaryOperator::ShiftRight | BinaryOperator::BitAnd
                    | BinaryOperator::BitOr   | BinaryOperator::BitXor => {
                        Ok(LunaType::Number)
                    }
                    BinaryOperator::Equal | BinaryOperator::NotEqual
                    | BinaryOperator::Less | BinaryOperator::LessEqual
                    | BinaryOperator::Greater | BinaryOperator::GreaterEqual
                    | BinaryOperator::And | BinaryOperator::Or => {
                        Ok(LunaType::Bool)
                    }
                }
            }

            Expr::UnaryOp { op, operand, .. } => {
                let _ = self.infer_expr(operand)?;
                match op {
                    UnaryOperator::Not    => Ok(LunaType::Bool),
                    UnaryOperator::Negate | UnaryOperator::BitNot => Ok(LunaType::Number),
                }
            }

            Expr::Assign { target, value, line, col } => {
                self.check_assignable(target, *line, *col)?;
                let vt = self.infer_expr(value)?;
                Ok(vt)
            }

            Expr::CompoundAssign { target, value, .. } => {
                let (line, col) = target.location();
                self.check_assignable(target, line, col)?;
                self.infer_expr(value)?;
                Ok(LunaType::Void)
            }

            Expr::Call { callee, args, line, col } => {
                if let Expr::Identifier { name, .. } = callee.as_ref() {
                    if let Some(fn_info) = self.lookup_fn(name).cloned() {
                        if fn_info.params.len() != args.len() {
                            return Err(LunaError::WrongArgumentCount {
                                fn_name: name.clone(),
                                expected: fn_info.params.len(),
                                found: args.len(),
                                line: *line, col: *col,
                            });
                        }
                        for arg in args {
                            self.infer_expr(arg)?;
                        }
                        return Ok(fn_info.return_type.clone());
                    }
                    if self.lookup_class(name).is_some() {
                        for arg in args { self.infer_expr(arg)?; }
                        return Ok(LunaType::Custom(name.clone()));
                    }
                }
                for arg in args { self.infer_expr(arg)?; }
                Ok(LunaType::Inferred)
            }

            Expr::MacroCall { args, .. } => {
                for arg in args { self.infer_expr(arg)?; }
                Ok(LunaType::Void)
            }

            Expr::MethodCall { object, args, .. } => {
                self.infer_expr(object)?;
                for arg in args { self.infer_expr(arg)?; }
                Ok(LunaType::Inferred)
            }

            Expr::FieldAccess { object, .. } => {
                self.infer_expr(object)?;
                Ok(LunaType::Inferred)
            }

            Expr::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object)?;
                self.infer_expr(index)?;
                match obj_ty {
                    LunaType::Array(inner) => Ok(*inner),
                    _ => Ok(LunaType::Inferred),
                }
            }

            Expr::Cast { value, target_type, .. } => {
                self.infer_expr(value)?;
                Ok(target_type.clone())
            }

            Expr::Lambda { params, body, .. } => {
                self.push_scope();
                for p in params {
                    self.declare_var(&p.name, p.ty.clone(), p.mutable, 0, 0)?;
                }
                for stmt in body {
                    self.check_stmt(stmt)?;
                }
                self.pop_scope();
                Ok(LunaType::Inferred)
            }
        }
    }

    fn check_assignable(&mut self, expr: &Expr, line: usize, col: usize) -> LunaResult<()> {
        match expr {
            Expr::Identifier { name, .. } => {
                if let Some(info) = self.lookup_var(name) {
                    if !info.mutable {
                        return Err(LunaError::ImmutableAssignment {
                            name: name.clone(), line, col,
                        });
                    }
                }
                Ok(())
            }
            Expr::FieldAccess { .. } | Expr::IndexAccess { .. } => Ok(()),
            _ => Err(LunaError::InvalidSyntax {
                message: "invalid assignment target".to_string(), line, col,
            }),
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> LunaResult<()> {
        match stmt {
            Stmt::Import { module, .. } => {
                self.imports.push(module.clone());
                Ok(())
            }

            Stmt::Use { .. } => Ok(()),

            Stmt::VarDecl { name, mutable, ty, initializer, line, col, .. } => {
                let resolved = if let Some(init) = initializer {
                    let it = self.infer_expr(init)?;
                    if *ty != LunaType::Inferred && !Self::types_compatible(ty, &it) {
                        return Err(LunaError::TypeMismatch {
                            expected: ty.display(), found: it.display(),
                            line: *line, col: *col,
                        });
                    }
                    if *ty == LunaType::Inferred { it } else { ty.clone() }
                } else {
                    ty.clone()
                };
                self.declare_var(name, resolved, *mutable, *line, *col)
            }

            Stmt::ConstDecl { name, ty, value, line, col } => {
                let vt = self.infer_expr(value)?;
                let resolved = if *ty == LunaType::Inferred { vt } else { ty.clone() };
                self.declare_var(name, resolved, false, *line, *col)
            }

            Stmt::FnDecl { name, params, return_type, body, .. } => {
                let param_types: Vec<LunaType> = params.iter().map(|p| p.ty.clone()).collect();
                self.declare_fn(name, param_types, return_type.clone());

                self.push_scope();
                self.fn_stack.push(return_type.clone());
                for p in params {
                    self.declare_var(&p.name, p.ty.clone(), p.mutable, 0, 0)?;
                }
                for s in body {
                    self.check_stmt(s)?;
                }
                self.fn_stack.pop();
                self.pop_scope();
                Ok(())
            }

            Stmt::ClassDecl { name, params, members, line, col, .. } => {
                let mut fields  = Vec::new();
                let mut methods = HashMap::new();

                for m in members {
                    match m {
                        Stmt::VarDecl { name: fname, ty, .. } => {
                            fields.push((fname.clone(), ty.clone()));
                        }
                        Stmt::ConstDecl { name: fname, ty, .. } => {
                            fields.push((fname.clone(), ty.clone()));
                        }
                        Stmt::FnDecl { name: mname, params: mparams, return_type, .. } => {
                            let pt: Vec<LunaType> = mparams.iter().map(|p| p.ty.clone()).collect();
                            methods.insert(mname.clone(), FnInfo { params: pt, return_type: return_type.clone() });
                        }
                        _ => {}
                    }
                }

                self.declare_class(name, ClassInfo { fields, methods });

                self.push_scope();
                for p in params {
                    self.declare_var(&p.name, p.ty.clone(), p.mutable, *line, *col)?;
                }
                for m in members {
                    self.check_stmt(m)?;
                }
                self.pop_scope();
                Ok(())
            }

            Stmt::NamespaceDecl { body, .. } => {
                self.push_scope();
                for s in body { self.check_stmt(s)?; }
                self.pop_scope();
                Ok(())
            }

            Stmt::Return { value, line, col } => {
                if self.fn_stack.is_empty() {
                    return Err(LunaError::ReturnOutsideFunction { line: *line, col: *col });
                }
                if let Some(expr) = value {
                    let vt = self.infer_expr(expr)?;
                    let expected = self.fn_stack.last().cloned().unwrap_or(LunaType::Void);
                    if !Self::types_compatible(&expected, &vt) {
                        return Err(LunaError::TypeMismatch {
                            expected: expected.display(), found: vt.display(),
                            line: *line, col: *col,
                        });
                    }
                }
                Ok(())
            }

            Stmt::Break { .. } | Stmt::Continue { .. } => Ok(()),

            Stmt::If { condition, then_body, else_if_branches, else_body, .. } => {
                self.infer_expr(condition)?;
                self.push_scope();
                for s in then_body { self.check_stmt(s)?; }
                self.pop_scope();
                for (cond, body) in else_if_branches {
                    self.infer_expr(cond)?;
                    self.push_scope();
                    for s in body { self.check_stmt(s)?; }
                    self.pop_scope();
                }
                if let Some(else_stmts) = else_body {
                    self.push_scope();
                    for s in else_stmts { self.check_stmt(s)?; }
                    self.pop_scope();
                }
                Ok(())
            }

            Stmt::While { condition, body, .. } => {
                self.infer_expr(condition)?;
                self.push_scope();
                for s in body { self.check_stmt(s)?; }
                self.pop_scope();
                Ok(())
            }

            Stmt::For { variable, iterable, body, line, col } => {
                let iter_ty = self.infer_expr(iterable)?;
                let elem_ty = match iter_ty {
                    LunaType::Array(inner) => *inner,
                    _ => LunaType::Inferred,
                };
                self.push_scope();
                self.declare_var(variable, elem_ty, false, *line, *col)?;
                for s in body { self.check_stmt(s)?; }
                self.pop_scope();
                Ok(())
            }

            Stmt::Loop { body, .. } => {
                self.push_scope();
                for s in body { self.check_stmt(s)?; }
                self.pop_scope();
                Ok(())
            }

            Stmt::ExprStmt { expr, .. } => {
                self.infer_expr(expr)?;
                Ok(())
            }
        }
    }

    pub fn check(&mut self, program: &Program) -> LunaResult<()> {
        for stmt in &program.statements {
            match stmt {
                Stmt::FnDecl { name, params, return_type, .. } => {
                    let pt: Vec<LunaType> = params.iter().map(|p| p.ty.clone()).collect();
                    self.declare_fn(name, pt, return_type.clone());
                }
                Stmt::ClassDecl { name, .. } => {
                    self.declare_class(name, ClassInfo { fields: vec![], methods: HashMap::new() });
                }
                _ => {}
            }
        }

        for stmt in &program.statements {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }
}
