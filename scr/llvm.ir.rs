use std::collections::HashMap;
use crate::ast::nodes::*;
use crate::errors::{LunaError, LunaResult};

struct Counter {
    n: usize,
}
impl Counter {
    fn new() -> Self { Counter { n: 0 } }
    fn next(&mut self) -> usize { let r = self.n; self.n += 1; r }
    fn next_reg(&mut self) -> String { format!("%r{}", self.next()) }
    fn next_label(&mut self, prefix: &str) -> String { format!("{}{}", prefix, self.next()) }
}

struct StringPool {
    entries: Vec<(usize, String)>,
}
impl StringPool {
    fn new() -> Self { StringPool { entries: vec![] } }

    fn intern(&mut self, s: &str) -> usize {
        for (id, existing) in &self.entries {
            if existing == s { return *id; }
        }
        let id = self.entries.len();
        self.entries.push((id, s.to_string()));
        id
    }

    fn emit_globals(&self) -> String {
        let mut out = String::new();
        for (id, content) in &self.entries {
            let escaped = escape_string_for_llvm(content);
            let len = content.len() + 1;
            out.push_str(&format!(
                "@.str.{id} = private unnamed_addr constant [{len} x i8] c\"{escaped}\\00\", align 1\n"
            ));
        }
        out
    }
}

fn escape_string_for_llvm(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"'  => out.push_str("\\22"),
            '\\' => out.push_str("\\5C"),
            '\n' => out.push_str("\\0A"),
            '\t' => out.push_str("\\09"),
            '\r' => out.push_str("\\0D"),
            '\0' => out.push_str("\\00"),
            c if (c as u32) < 32 => out.push_str(&format!("\\{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

struct LocalEnv {
    vars: HashMap<String, String>,
}
impl LocalEnv {
    fn new() -> Self { LocalEnv { vars: HashMap::new() } }
    fn set(&mut self, name: &str, ptr: String) { self.vars.insert(name.to_string(), ptr); }
    fn get(&self, name: &str) -> Option<&String> { self.vars.get(name) }
}

fn luna_type_to_llvm(ty: &LunaType) -> &'static str {
    match ty {
        LunaType::Number | LunaType::F64 => "double",
        LunaType::F32    => "float",
        LunaType::I32    => "i32",
        LunaType::I64    => "i64",
        LunaType::U8     => "i8",
        LunaType::Usize  => "i64",
        LunaType::Bool   => "i1",
        LunaType::String => "i8*",
        LunaType::Void   => "void",
        _                => "i8*",
    }
}

pub struct LlvmIrGenerator {
    strings:     StringPool,
    counter:     Counter,
    functions:   Vec<String>,
    class_layouts: HashMap<String, Vec<(String, String)>>,
}

impl LlvmIrGenerator {
    pub fn new() -> Self {
        LlvmIrGenerator {
            strings:       StringPool::new(),
            counter:       Counter::new(),
            functions:     Vec::new(),
            class_layouts: HashMap::new(),
        }
    }

    fn reg(&mut self) -> String { self.counter.next_reg() }
    fn label(&mut self, prefix: &str) -> String { self.counter.next_label(prefix) }

    fn str_ptr(&mut self, id: usize, len: usize, out: &mut String) -> String {
        let reg = self.reg();
        out.push_str(&format!(
            "  {reg} = getelementptr inbounds [{len} x i8], [{len} x i8]* @.str.{id}, i64 0, i64 0\n"
        ));
        reg
    }

    fn gen_expr(
        &mut self,
        expr: &Expr,
        env: &mut LocalEnv,
    ) -> LunaResult<(String, &'static str, String)> {
        let mut out = String::new();

        match expr {
            Expr::NumberLit { value, .. } => {
                let reg = self.reg();
                out.push_str(&format!("  {reg} = fadd double 0.0, {value:e}\n"));
                Ok((reg, "double", out))
            }

            Expr::BoolLit { value, .. } => {
                let reg = self.reg();
                let v = if *value { 1 } else { 0 };
                out.push_str(&format!("  {reg} = add i1 0, {v}\n"));
                Ok((reg, "i1", out))
            }

            Expr::StringLit { value, .. } => {
                let id = self.strings.intern(value);
                let len = value.len() + 1;
                let ptr = self.str_ptr(id, len, &mut out);
                Ok((ptr, "i8*", out))
            }

            Expr::FStringLit { template, .. } => {
                let id = self.strings.intern(template);
                let len = template.len() + 1;
                let ptr = self.str_ptr(id, len, &mut out);
                Ok((ptr, "i8*", out))
            }

            Expr::NullLit { .. } => {
                let reg = self.reg();
                out.push_str(&format!("  {reg} = bitcast i8* null to i8*\n"));
                Ok((reg, "i8*", out))
            }

            Expr::Identifier { name, line, col } => {
                if let Some(ptr) = env.get(name).cloned() {
                    let llvm_ty = if ptr.ends_with(".bptr") { "i1" }
                                  else if ptr.ends_with(".sptr") { "i8*" }
                                  else { "double" };
                    let reg = self.reg();
                    out.push_str(&format!("  {reg} = load {llvm_ty}, {llvm_ty}* {ptr}, align 8\n"));
                    Ok((reg, llvm_ty, out))
                } else {
                    Err(LunaError::CodegenError {
                        message: format!("undefined variable `{name}` at {line}:{col}"),
                    })
                }
            }

            Expr::BinaryOp { left, op, right, .. } => {
                let (lreg, lty, lcode) = self.gen_expr(left, env)?;
                let (rreg, rty, rcode) = self.gen_expr(right, env)?;
                out.push_str(&lcode);
                out.push_str(&rcode);

                let result = self.reg();

                match op {
                    BinaryOperator::Add => {
                        if lty == "i8*" {
                            out.push_str(&format!(
                                "  {result} = call i8* @luna_strcat(i8* {lreg}, i8* {rreg})\n"
                            ));
                            Ok((result, "i8*", out))
                        } else {
                            out.push_str(&format!("  {result} = fadd double {lreg}, {rreg}\n"));
                            Ok((result, "double", out))
                        }
                    }
                    BinaryOperator::Subtract => {
                        out.push_str(&format!("  {result} = fsub double {lreg}, {rreg}\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::Multiply => {
                        out.push_str(&format!("  {result} = fmul double {lreg}, {rreg}\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::Divide => {
                        out.push_str(&format!("  {result} = fdiv double {lreg}, {rreg}\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::Modulo => {
                        out.push_str(&format!("  {result} = frem double {lreg}, {rreg}\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::Power => {
                        out.push_str(&format!(
                            "  {result} = call double @llvm.pow.f64(double {lreg}, double {rreg})\n"
                        ));
                        Ok((result, "double", out))
                    }

                    BinaryOperator::Equal => {
                        let cmp = if lty == "i8*" {
                            let cmpres = self.reg();
                            out.push_str(&format!(
                                "  {cmpres} = call i32 @strcmp(i8* {lreg}, i8* {rreg})\n"
                            ));
                            out.push_str(&format!(
                                "  {result} = icmp eq i32 {cmpres}, 0\n"
                            ));
                            return Ok((result, "i1", out));
                        } else if lty == "i1" { "icmp eq i1" } else { "fcmp oeq double" };
                        out.push_str(&format!("  {result} = {cmp} {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }
                    BinaryOperator::NotEqual => {
                        let cmp = if lty == "i1" { "icmp ne i1" } else { "fcmp one double" };
                        out.push_str(&format!("  {result} = {cmp} {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }
                    BinaryOperator::Less => {
                        out.push_str(&format!("  {result} = fcmp olt double {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }
                    BinaryOperator::LessEqual => {
                        out.push_str(&format!("  {result} = fcmp ole double {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }
                    BinaryOperator::Greater => {
                        out.push_str(&format!("  {result} = fcmp ogt double {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }
                    BinaryOperator::GreaterEqual => {
                        out.push_str(&format!("  {result} = fcmp oge double {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }

                    BinaryOperator::And => {
                        out.push_str(&format!("  {result} = and i1 {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }
                    BinaryOperator::Or => {
                        out.push_str(&format!("  {result} = or i1 {lreg}, {rreg}\n"));
                        Ok((result, "i1", out))
                    }

                    BinaryOperator::BitAnd => {
                        let conv_l = self.reg();
                        let conv_r = self.reg();
                        out.push_str(&format!("  {conv_l} = fptosi double {lreg} to i64\n"));
                        out.push_str(&format!("  {conv_r} = fptosi double {rreg} to i64\n"));
                        let tmp = self.reg();
                        out.push_str(&format!("  {tmp} = and i64 {conv_l}, {conv_r}\n"));
                        out.push_str(&format!("  {result} = sitofp i64 {tmp} to double\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::BitOr => {
                        let conv_l = self.reg(); let conv_r = self.reg();
                        out.push_str(&format!("  {conv_l} = fptosi double {lreg} to i64\n"));
                        out.push_str(&format!("  {conv_r} = fptosi double {rreg} to i64\n"));
                        let tmp = self.reg();
                        out.push_str(&format!("  {tmp} = or i64 {conv_l}, {conv_r}\n"));
                        out.push_str(&format!("  {result} = sitofp i64 {tmp} to double\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::BitXor => {
                        let conv_l = self.reg(); let conv_r = self.reg();
                        out.push_str(&format!("  {conv_l} = fptosi double {lreg} to i64\n"));
                        out.push_str(&format!("  {conv_r} = fptosi double {rreg} to i64\n"));
                        let tmp = self.reg();
                        out.push_str(&format!("  {tmp} = xor i64 {conv_l}, {conv_r}\n"));
                        out.push_str(&format!("  {result} = sitofp i64 {tmp} to double\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::ShiftLeft => {
                        let conv_l = self.reg(); let conv_r = self.reg();
                        out.push_str(&format!("  {conv_l} = fptosi double {lreg} to i64\n"));
                        out.push_str(&format!("  {conv_r} = fptosi double {rreg} to i64\n"));
                        let tmp = self.reg();
                        out.push_str(&format!("  {tmp} = shl i64 {conv_l}, {conv_r}\n"));
                        out.push_str(&format!("  {result} = sitofp i64 {tmp} to double\n"));
                        Ok((result, "double", out))
                    }
                    BinaryOperator::ShiftRight => {
                        let conv_l = self.reg(); let conv_r = self.reg();
                        out.push_str(&format!("  {conv_l} = fptosi double {lreg} to i64\n"));
                        out.push_str(&format!("  {conv_r} = fptosi double {rreg} to i64\n"));
                        let tmp = self.reg();
                        out.push_str(&format!("  {tmp} = ashr i64 {conv_l}, {conv_r}\n"));
                        out.push_str(&format!("  {result} = sitofp i64 {tmp} to double\n"));
                        Ok((result, "double", out))
                    }
                }
            }

            Expr::UnaryOp { op, operand, .. } => {
                let (oreg, oty, ocode) = self.gen_expr(operand, env)?;
                out.push_str(&ocode);
                let result = self.reg();
                match op {
                    UnaryOperator::Negate => {
                        out.push_str(&format!("  {result} = fneg double {oreg}\n"));
                        Ok((result, "double", out))
                    }
                    UnaryOperator::Not => {
                        out.push_str(&format!("  {result} = xor i1 {oreg}, 1\n"));
                        Ok((result, "i1", out))
                    }
                    UnaryOperator::BitNot => {
                        let tmp = self.reg();
                        out.push_str(&format!("  {tmp} = fptosi double {oreg} to i64\n"));
                        let tmp2 = self.reg();
                        out.push_str(&format!("  {tmp2} = xor i64 {tmp}, -1\n"));
                        out.push_str(&format!("  {result} = sitofp i64 {tmp2} to double\n"));
                        Ok((result, "double", out))
                    }
                }
            }

            Expr::MacroCall { name, args, .. } => {
                match name.as_str() {
                    "println" | "print" => {
                        let newline = name == "println";
                        let mut fmt_parts = Vec::new();
                        let mut arg_regs  = Vec::new();

                        for arg in args {
                            let (reg, ty, code) = self.gen_expr(arg, env)?;
                            out.push_str(&code);
                            match ty {
                                "double" => {
                                    fmt_parts.push("%g".to_string());
                                    arg_regs.push((reg, "double"));
                                }
                                "i1" => {
                                    let casted = self.reg();
                                    out.push_str(&format!(
                                        "  {casted} = zext i1 {reg} to i32\n"
                                    ));
                                    let true_id  = self.strings.intern("true");
                                    let false_id = self.strings.intern("false");
                                    let true_ptr  = self.reg();
                                    let false_ptr = self.reg();
                                    let tlen = 5; let flen = 6;
                                    out.push_str(&format!(
                                        "  {true_ptr} = getelementptr inbounds [{tlen} x i8], [{tlen} x i8]* @.str.{true_id}, i64 0, i64 0\n"
                                    ));
                                    out.push_str(&format!(
                                        "  {false_ptr} = getelementptr inbounds [{flen} x i8], [{flen} x i8]* @.str.{false_id}, i64 0, i64 0\n"
                                    ));
                                    let sel = self.reg();
                                    out.push_str(&format!(
                                        "  {sel} = select i1 {reg}, i8* {true_ptr}, i8* {false_ptr}\n"
                                    ));
                                    fmt_parts.push("%s".to_string());
                                    arg_regs.push((sel, "i8*"));
                                }
                                _ => {
                                    fmt_parts.push("%s".to_string());
                                    arg_regs.push((reg, "i8*"));
                                }
                            }
                        }

                        let fmt_str = if newline {
                            format!("{}\n", fmt_parts.join(""))
                        } else {
                            fmt_parts.join("")
                        };

                        self.strings.intern("true");
                        self.strings.intern("false");

                        let fmt_id = self.strings.intern(&fmt_str);
                        let fmt_len = fmt_str.len() + 1;
                        let fmt_ptr = self.str_ptr(fmt_id, fmt_len, &mut out);

                        let call_result = self.reg();
                        let mut arg_list = format!("i8* noundef {fmt_ptr}");
                        for (r, t) in &arg_regs {
                            arg_list.push_str(&format!(", {t} noundef {r}"));
                        }
                        out.push_str(&format!(
                            "  {call_result} = call i32 (i8*, ...) @printf({arg_list})\n"
                        ));
                        Ok((call_result, "i32", out))
                    }
                    "eprint" | "eprintln" => {
                        let call_result = self.reg();
                        out.push_str(&format!("  {call_result} = add i32 0, 0\n"));
                        Ok((call_result, "i32", out))
                    }
                    _ => {
                        let reg = self.reg();
                        out.push_str(&format!("  {reg} = add i32 0, 0  ; macro {name}\n"));
                        Ok((reg, "i32", out))
                    }
                }
            }

            Expr::Call { callee, args, .. } => {
                let mut arg_regs = Vec::new();
                for arg in args {
                    let (reg, ty, code) = self.gen_expr(arg, env)?;
                    out.push_str(&code);
                    arg_regs.push((reg, ty));
                }

                let fn_name = match callee.as_ref() {
                    Expr::Identifier { name, .. } => format!("@{name}"),
                    _ => "@unknown".to_string(),
                };

                let result = self.reg();
                let arg_list: String = arg_regs.iter()
                    .map(|(r, t)| format!("{t} {r}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "  {result} = call double {fn_name}({arg_list})\n"
                ));
                Ok((result, "double", out))
            }

            Expr::MethodCall { object, method, args, .. } => {
                let (oreg, _oty, ocode) = self.gen_expr(object, env)?;
                out.push_str(&ocode);
                let mut arg_regs = vec![(oreg, "i8*")];
                for arg in args {
                    let (r, t, c) = self.gen_expr(arg, env)?;
                    out.push_str(&c);
                    arg_regs.push((r, t));
                }
                let result = self.reg();
                let arg_list: String = arg_regs.iter()
                    .map(|(r, t)| format!("{t} {r}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "  {result} = call i8* @luna_method_{method}({arg_list})\n"
                ));
                Ok((result, "i8*", out))
            }

            Expr::Grouped(inner) => self.gen_expr(inner, env),

            _ => {
                let reg = self.reg();
                out.push_str(&format!("  {reg} = add i32 0, 0  ; unimplemented expr\n"));
                Ok((reg, "i32", out))
            }
        }
    }

    fn gen_stmt(&mut self, stmt: &Stmt, env: &mut LocalEnv) -> LunaResult<String> {
        let mut out = String::new();

        match stmt {
            Stmt::Import { .. } | Stmt::Use { .. } => {}

            Stmt::VarDecl { name, initializer, ty, .. } => {
                let llvm_ty = luna_type_to_llvm(ty);
                let ptr_name = match llvm_ty {
                    "i1"  => format!("%{name}.bptr"),
                    "i8*" => format!("%{name}.sptr"),
                    _     => format!("%{name}.ptr"),
                };
                out.push_str(&format!("  {ptr_name} = alloca {llvm_ty}, align 8\n"));

                if let Some(init) = initializer {
                    let (reg, rty, code) = self.gen_expr(init, env)?;
                    out.push_str(&code);
                    out.push_str(&format!("  store {rty} {reg}, {rty}* {ptr_name}, align 8\n"));
                }
                env.set(name, ptr_name);
            }

            Stmt::ConstDecl { name, ty, value, .. } => {
                let llvm_ty = luna_type_to_llvm(ty);
                let ptr_name = format!("%{name}.ptr");
                out.push_str(&format!("  {ptr_name} = alloca {llvm_ty}, align 8\n"));
                let (reg, rty, code) = self.gen_expr(value, env)?;
                out.push_str(&code);
                out.push_str(&format!("  store {rty} {reg}, {rty}* {ptr_name}, align 8\n"));
                env.set(name, ptr_name);
            }

            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    let (reg, ty, code) = self.gen_expr(expr, env)?;
                    out.push_str(&code);
                    out.push_str(&format!("  ret {ty} {reg}\n"));
                } else {
                    out.push_str("  ret void\n");
                }
            }

            Stmt::ExprStmt { expr, .. } => {
                let (_, _, code) = self.gen_expr(expr, env)?;
                out.push_str(&code);
            }

            Stmt::If { condition, then_body, else_if_branches, else_body, .. } => {
                let then_label = self.label("if.then.");
                let else_label = self.label("if.else.");
                let end_label  = self.label("if.end.");

                let (creg, _, ccode) = self.gen_expr(condition, env)?;
                out.push_str(&ccode);
                out.push_str(&format!(
                    "  br i1 {creg}, label %{then_label}, label %{else_label}\n"
                ));

                out.push_str(&format!("{then_label}:\n"));
                for s in then_body {
                    out.push_str(&self.gen_stmt(s, env)?);
                }
                out.push_str(&format!("  br label %{end_label}\n"));

                out.push_str(&format!("{else_label}:\n"));
                if !else_if_branches.is_empty() {
                    for (cond, body) in else_if_branches {
                        let ei_then = self.label("elif.then.");
                        let ei_next = self.label("elif.next.");
                        let (cr, _, cc) = self.gen_expr(cond, env)?;
                        out.push_str(&cc);
                        out.push_str(&format!("  br i1 {cr}, label %{ei_then}, label %{ei_next}\n"));
                        out.push_str(&format!("{ei_then}:\n"));
                        for s in body { out.push_str(&self.gen_stmt(s, env)?); }
                        out.push_str(&format!("  br label %{end_label}\n"));
                        out.push_str(&format!("{ei_next}:\n"));
                    }
                }
                if let Some(else_stmts) = else_body {
                    for s in else_stmts { out.push_str(&self.gen_stmt(s, env)?); }
                }
                out.push_str(&format!("  br label %{end_label}\n"));

                out.push_str(&format!("{end_label}:\n"));
            }

            Stmt::While { condition, body, .. } => {
                let header = self.label("while.header.");
                let body_l = self.label("while.body.");
                let exit   = self.label("while.exit.");

                out.push_str(&format!("  br label %{header}\n"));
                out.push_str(&format!("{header}:\n"));

                let (creg, _, ccode) = self.gen_expr(condition, env)?;
                out.push_str(&ccode);
                out.push_str(&format!(
                    "  br i1 {creg}, label %{body_l}, label %{exit}\n"
                ));

                out.push_str(&format!("{body_l}:\n"));
                for s in body { out.push_str(&self.gen_stmt(s, env)?); }
                out.push_str(&format!("  br label %{header}\n"));

                out.push_str(&format!("{exit}:\n"));
            }

            Stmt::Break { .. } => {
                let dummy = self.label("break.dummy.");
                out.push_str(&format!("  br label %{dummy}\n"));
                out.push_str(&format!("{dummy}:\n"));
            }
            Stmt::Continue { .. } => {
                let dummy = self.label("continue.dummy.");
                out.push_str(&format!("  br label %{dummy}\n"));
                out.push_str(&format!("{dummy}:\n"));
            }

            Stmt::FnDecl { .. } | Stmt::ClassDecl { .. } | Stmt::NamespaceDecl { .. } => {}

            Stmt::For { variable, iterable, body, line, col } => {
                let (ireg, _ity, icode) = self.gen_expr(iterable, env)?;
                out.push_str(&icode);
                let loop_var = format!("%{variable}.ptr");
                out.push_str(&format!("  {loop_var} = alloca double, align 8\n"));
                env.set(variable, loop_var);
            }
        }

        Ok(out)
    }

    fn gen_fn(&mut self, name: &str, params: &[Param], return_type: &LunaType, body: &[Stmt]) -> LunaResult<String> {
        let ret_ty  = luna_type_to_llvm(return_type);
        let param_str: String = params.iter()
            .map(|p| format!("{} %{}.arg", luna_type_to_llvm(&p.ty), p.name))
            .collect::<Vec<_>>()
            .join(", ");

        let mut out = format!("define {ret_ty} @{name}({param_str}) {{\n");
        out.push_str("entry:\n");

        let mut env = LocalEnv::new();

        for p in params {
            let llvm_ty = luna_type_to_llvm(&p.ty);
            let ptr = format!("%{}.ptr", p.name);
            out.push_str(&format!("  {ptr} = alloca {llvm_ty}, align 8\n"));
            out.push_str(&format!("  store {llvm_ty} %{}.arg, {llvm_ty}* {ptr}, align 8\n", p.name));
            env.set(&p.name, ptr);
        }

        let mut has_explicit_return = false;
        for stmt in body {
            if matches!(stmt, Stmt::Return { .. }) { has_explicit_return = true; }
            out.push_str(&self.gen_stmt(stmt, &mut env)?);
        }

        if !has_explicit_return {
            match return_type {
                LunaType::Void => out.push_str("  ret void\n"),
                _ => out.push_str(&format!("  ret {ret_ty} 0\n")),
            }
        }

        out.push_str("}\n");
        Ok(out)
    }

    pub fn generate(&mut self, program: &Program) -> LunaResult<String> {
        let mut fn_bodies = Vec::new();

        self.strings.intern("true");
        self.strings.intern("false");

        for stmt in &program.statements {
            match stmt {
                Stmt::FnDecl { name, params, return_type, body, .. } => {
                    let code = self.gen_fn(name, params, return_type, body)?;
                    fn_bodies.push(code);
                }
                Stmt::ClassDecl { name, members, .. } => {
                    for m in members {
                        if let Stmt::FnDecl { name: mname, params, return_type, body, .. } = m {
                            let full_name = if mname == "main" && name == "main" {
                                "luna_main".to_string()
                            } else {
                                format!("{name}_{mname}")
                            };
                            let code = self.gen_fn(&full_name, params, return_type, body)?;
                            fn_bodies.push(code);
                        }
                    }
                }
                _ => {}
            }
        }

        let mut ir = String::new();

        ir.push_str(&format!(
            "; Luna Script LLVM IR\n\
             ; Generated by lunasc v0.2.0\n\
             ; Source: {}\n\
             ;\n\
             ; Compile: clang {0} -o program  [after renaming to .ll]\n\
             ; Run IR:  lli {0}               [LLVM interpreter]\n\
             ; Optimize: opt -O2 -S -o opt.ll {0}\n\n",
            program.source_path,
        ));

        ir.push_str("target triple = \"x86_64-pc-linux-gnu\"\n\n");

        ir.push_str("declare i32 @printf(i8* noundef, ...)\n");
        ir.push_str("declare i32 @puts(i8* noundef)\n");
        ir.push_str("declare i32 @strcmp(i8* noundef, i8* noundef)\n");
        ir.push_str("declare i8* @strcpy(i8* noundef, i8* noundef)\n");
        ir.push_str("declare i8* @strcat(i8* noundef, i8* noundef)\n");
        ir.push_str("declare i8* @malloc(i64 noundef)\n");
        ir.push_str("declare void @free(i8* noundef)\n");
        ir.push_str("declare double @llvm.pow.f64(double, double)\n");
        ir.push_str("declare double @sqrt(double)\n");
        ir.push_str("declare double @fabs(double)\n");
        ir.push_str("declare double @floor(double)\n");
        ir.push_str("declare double @ceil(double)\n");
        ir.push_str("\n");

        let str_globals_placeholder = "<<STRING_GLOBALS>>";
        ir.push_str(str_globals_placeholder);
        ir.push_str("\n");

        for body in &fn_bodies {
            ir.push_str(body);
            ir.push_str("\n");
        }

        ir.push_str("define i32 @main() {\n");
        ir.push_str("entry:\n");
        ir.push_str("  call void @luna_main()\n");
        ir.push_str("  ret i32 0\n");
        ir.push_str("}\n");

        let str_globals = format!(
            "{}\n",
            self.strings.emit_globals()
        );
        let ir = ir.replace(str_globals_placeholder, &str_globals);

        Ok(ir)
    }
}
