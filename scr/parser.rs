use crate::ast::nodes::*;
use crate::errors::{LunaError, LunaResult};
use crate::lexer::token::{Token, TokenInfo};

pub struct Parser {
    tokens: Vec<TokenInfo>,
    pos: usize,
    source_path: String,
}

impl Parser {
    pub fn new(tokens: Vec<TokenInfo>, source_path: impl Into<String>) -> Self {
        Parser { tokens, pos: 0, source_path: source_path.into() }
    }

    fn current(&self) -> &TokenInfo {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_n(&self, n: usize) -> &TokenInfo {
        let idx = (self.pos + n).min(self.tokens.len() - 1);
        &self.tokens[idx]
    }

    fn advance(&mut self) -> &TokenInfo {
        let idx = self.pos.min(self.tokens.len() - 1);
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        &self.tokens[idx]
    }

    fn expect(&mut self, expected: Token) -> LunaResult<TokenInfo> {
        let tok = self.current().clone();
        if tok.token == expected {
            self.advance();
            Ok(tok)
        } else {
            Err(LunaError::UnexpectedToken {
                expected: expected.to_string(),
                found: tok.token.to_string(),
                line: tok.line,
                col: tok.col,
            })
        }
    }

    fn check(&self, tok: &Token) -> bool {
        &self.current().token == tok
    }

    fn check_any(&self, toks: &[Token]) -> bool {
        toks.iter().any(|t| self.check(t))
    }

    fn match_token(&mut self, tok: &Token) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().token, Token::Eof)
    }

    fn loc(&self) -> (usize, usize) {
        (self.current().line, self.current().col)
    }

    fn parse_type(&mut self) -> LunaResult<LunaType> {
        let tok = self.advance().clone();
        let mut base = match &tok.token {
            Token::TypeString  => LunaType::String,
            Token::TypeNumber  => LunaType::Number,
            Token::TypeBool    => LunaType::Bool,
            Token::TypeVoid    => LunaType::Void,
            Token::TypeI32     => LunaType::I32,
            Token::TypeI64     => LunaType::I64,
            Token::TypeF32     => LunaType::F32,
            Token::TypeF64     => LunaType::F64,
            Token::TypeU8      => LunaType::U8,
            Token::TypeUsize   => LunaType::Usize,
            Token::Identifier(name) => {
                let name = name.clone();
                if self.check(&Token::Less) {
                    self.advance();
                    let mut args = vec![self.parse_type()?];
                    while self.match_token(&Token::Comma) {
                        args.push(self.parse_type()?);
                    }
                    self.expect(Token::Greater)?;
                    LunaType::Generic(name, args)
                } else {
                    LunaType::Custom(name)
                }
            }
            Token::LeftBracket => {
                let inner = self.parse_type()?;
                self.expect(Token::RightBracket)?;
                LunaType::Array(Box::new(inner))
            }
            Token::Fn => {
                self.expect(Token::LeftParen)?;
                let mut params = Vec::new();
                while !self.check(&Token::RightParen) && !self.at_eof() {
                    params.push(self.parse_type()?);
                    if !self.match_token(&Token::Comma) { break; }
                }
                self.expect(Token::RightParen)?;
                let ret = if self.match_token(&Token::Arrow) {
                    self.parse_type()?
                } else {
                    LunaType::Void
                };
                LunaType::Function(params, Box::new(ret))
            }
            other => {
                return Err(LunaError::InvalidSyntax {
                    message: format!("expected a type, found `{other}`"),
                    line: tok.line,
                    col: tok.col,
                })
            }
        };

        while self.check(&Token::QuestionMark) {
            self.advance();
            base = LunaType::Option(Box::new(base));
        }

        Ok(base)
    }

    fn parse_params(&mut self) -> LunaResult<Vec<Param>> {
        self.expect(Token::LeftParen)?;
        let mut params = Vec::new();
        while !self.check(&Token::RightParen) && !self.at_eof() {
            let mutable = self.match_token(&Token::Mut);
            let name_tok = self.advance().clone();
            let name = match &name_tok.token {
                Token::Identifier(n) => n.clone(),
                other => {
                    return Err(LunaError::InvalidSyntax {
                        message: format!("expected parameter name, found `{other}`"),
                        line: name_tok.line,
                        col: name_tok.col,
                    })
                }
            };
            self.expect(Token::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.match_token(&Token::Equals) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            params.push(Param { name, ty, default, mutable });
            if !self.match_token(&Token::Comma) { break; }
        }
        self.expect(Token::RightParen)?;
        Ok(params)
    }

    fn parse_expr(&mut self) -> LunaResult<Expr> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> LunaResult<Expr> {
        let expr = self.parse_or()?;
        let (line, col) = self.loc();

        let compound = if self.check(&Token::PlusEquals) {
            Some(CompoundOp::AddAssign)
        } else if self.check(&Token::MinusEquals) {
            Some(CompoundOp::SubAssign)
        } else if self.check(&Token::StarEquals) {
            Some(CompoundOp::MulAssign)
        } else if self.check(&Token::SlashEquals) {
            Some(CompoundOp::DivAssign)
        } else {
            None
        };
        if let Some(op) = compound {
            self.advance();
            let value = self.parse_or()?;
            return Ok(Expr::CompoundAssign {
                target: Box::new(expr),
                op,
                value: Box::new(value),
                line,
                col,
            });
        }

        if self.check(&Token::Equals) {
            self.advance();
            let value = self.parse_or()?;
            return Ok(Expr::Assign {
                target: Box::new(expr),
                value: Box::new(value),
                line,
                col,
            });
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_and()?;
        while self.check(&Token::Or) {
            let (line, col) = self.loc();
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinaryOp { left: Box::new(left), op: BinaryOperator::Or, right: Box::new(right), line, col };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_equality()?;
        while self.check(&Token::And) {
            let (line, col) = self.loc();
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinaryOp { left: Box::new(left), op: BinaryOperator::And, right: Box::new(right), line, col };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = if self.check(&Token::EqualsEquals) { Some(BinaryOperator::Equal) }
                     else if self.check(&Token::BangEquals) { Some(BinaryOperator::NotEqual) }
                     else { None };
            if let Some(op) = op {
                let (line, col) = self.loc();
                self.advance();
                let right = self.parse_comparison()?;
                left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), line, col };
            } else { break; }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_bitwise()?;
        loop {
            let op = if self.check(&Token::Less) { Some(BinaryOperator::Less) }
                     else if self.check(&Token::LessEquals) { Some(BinaryOperator::LessEqual) }
                     else if self.check(&Token::Greater) { Some(BinaryOperator::Greater) }
                     else if self.check(&Token::GreaterEquals) { Some(BinaryOperator::GreaterEqual) }
                     else { None };
            if let Some(op) = op {
                let (line, col) = self.loc();
                self.advance();
                let right = self.parse_bitwise()?;
                left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), line, col };
            } else { break; }
        }
        Ok(left)
    }

    fn parse_bitwise(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = if self.check(&Token::Ampersand)  { Some(BinaryOperator::BitAnd) }
                     else if self.check(&Token::Pipe)  { Some(BinaryOperator::BitOr) }
                     else if self.check(&Token::Caret) { Some(BinaryOperator::BitXor) }
                     else if self.check(&Token::ShiftLeft) { Some(BinaryOperator::ShiftLeft) }
                     else if self.check(&Token::ShiftRight) { Some(BinaryOperator::ShiftRight) }
                     else { None };
            if let Some(op) = op {
                let (line, col) = self.loc();
                self.advance();
                let right = self.parse_additive()?;
                left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), line, col };
            } else { break; }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = if self.check(&Token::Plus) { Some(BinaryOperator::Add) }
                     else if self.check(&Token::Minus) { Some(BinaryOperator::Subtract) }
                     else { None };
            if let Some(op) = op {
                let (line, col) = self.loc();
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), line, col };
            } else { break; }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> LunaResult<Expr> {
        let mut left = self.parse_power()?;
        loop {
            let op = if self.check(&Token::Star)    { Some(BinaryOperator::Multiply) }
                     else if self.check(&Token::Slash)   { Some(BinaryOperator::Divide) }
                     else if self.check(&Token::Percent) { Some(BinaryOperator::Modulo) }
                     else { None };
            if let Some(op) = op {
                let (line, col) = self.loc();
                self.advance();
                let right = self.parse_power()?;
                left = Expr::BinaryOp { left: Box::new(left), op, right: Box::new(right), line, col };
            } else { break; }
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> LunaResult<Expr> {
        let base = self.parse_unary()?;
        if self.check(&Token::StarStar) {
            let (line, col) = self.loc();
            self.advance();
            let exp = self.parse_power()?;
            return Ok(Expr::BinaryOp {
                left: Box::new(base), op: BinaryOperator::Power,
                right: Box::new(exp), line, col,
            });
        }
        Ok(base)
    }

    fn parse_unary(&mut self) -> LunaResult<Expr> {
        let (line, col) = self.loc();
        if self.check(&Token::Bang) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::UnaryOp { op: UnaryOperator::Not, operand: Box::new(operand), line, col });
        }
        if self.check(&Token::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::UnaryOp { op: UnaryOperator::Negate, operand: Box::new(operand), line, col });
        }
        if self.check(&Token::Tilde) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::UnaryOp { op: UnaryOperator::BitNot, operand: Box::new(operand), line, col });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> LunaResult<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(&Token::Dot) {
                self.advance();
                let member_tok = self.advance().clone();
                let (line, col) = (member_tok.line, member_tok.col);
                let member = match &member_tok.token {
                    Token::Identifier(n) => n.clone(),
                    other => {
                        return Err(LunaError::InvalidSyntax {
                            message: format!("expected member name, found `{other}`"),
                            line, col,
                        })
                    }
                };

                if self.check(&Token::Bang) {
                    self.advance();
                    self.expect(Token::LeftParen)?;
                    let args = self.parse_arg_list()?;
                    self.expect(Token::RightParen)?;
                    expr = Expr::MacroCall {
                        receiver: Some(Box::new(expr)),
                        name: member, args, line, col,
                    };
                    continue;
                }

                if self.check(&Token::LeftParen) {
                    self.advance();
                    let args = self.parse_arg_list()?;
                    self.expect(Token::RightParen)?;
                    expr = Expr::MethodCall { object: Box::new(expr), method: member, args, line, col };
                    continue;
                }

                expr = Expr::FieldAccess { object: Box::new(expr), field: member, line, col };
                continue;
            }

            if self.check(&Token::LeftParen) {
                let (line, col) = self.loc();
                self.advance();
                let args = self.parse_arg_list()?;
                self.expect(Token::RightParen)?;
                expr = Expr::Call { callee: Box::new(expr), args, line, col };
                continue;
            }

            if self.check(&Token::LeftBracket) {
                let (line, col) = self.loc();
                self.advance();
                let index = self.parse_expr()?;
                self.expect(Token::RightBracket)?;
                expr = Expr::IndexAccess { object: Box::new(expr), index: Box::new(index), line, col };
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn parse_arg_list(&mut self) -> LunaResult<Vec<Expr>> {
        let mut args = Vec::new();
        while !self.check(&Token::RightParen) && !self.at_eof() {
            args.push(self.parse_expr()?);
            if !self.match_token(&Token::Comma) { break; }
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> LunaResult<Expr> {
        let tok = self.current().clone();
        let (line, col) = (tok.line, tok.col);

        match tok.token.clone() {
            Token::StringLiteral(s) => {
                self.advance();
                Ok(Expr::StringLit { value: s, line, col })
            }
            Token::FStringLiteral(s) => {
                self.advance();
                Ok(Expr::FStringLit { template: s, line, col })
            }
            Token::NumberLiteral(n) => {
                self.advance();
                Ok(Expr::NumberLit { value: n, line, col })
            }
            Token::BoolLiteral(b) => {
                self.advance();
                Ok(Expr::BoolLit { value: b, line, col })
            }
            Token::Null => {
                self.advance();
                Ok(Expr::NullLit { line, col })
            }
            Token::Identifier(name) => {
                self.advance();
                Ok(Expr::Identifier { name, line, col })
            }
            Token::LeftParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RightParen)?;
                Ok(Expr::Grouped(Box::new(expr)))
            }
            Token::LeftBracket => {
                self.advance();
                let mut elements = Vec::new();
                while !self.check(&Token::RightBracket) && !self.at_eof() {
                    elements.push(self.parse_expr()?);
                    if !self.match_token(&Token::Comma) { break; }
                }
                self.expect(Token::RightBracket)?;
                Ok(Expr::ArrayLit { elements, line, col })
            }
            Token::Fn => {
                self.advance();
                let params = self.parse_params()?;
                let body = self.parse_block()?;
                Ok(Expr::Lambda { params, body, line, col })
            }
            other => Err(LunaError::InvalidSyntax {
                message: format!("expected an expression, found `{other}`"),
                line,
                col,
            }),
        }
    }

    fn parse_block(&mut self) -> LunaResult<Vec<Stmt>> {
        self.expect(Token::LeftBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&Token::RightBrace) && !self.at_eof() {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(Token::RightBrace)?;
        Ok(stmts)
    }

    fn parse_modifiers(&mut self) -> (bool, bool, bool) {
        let mut is_pub    = false;
        let mut is_static = false;
        let mut is_async  = false;
        loop {
            if self.match_token(&Token::Pub)    { is_pub    = true; }
            else if self.match_token(&Token::Static) { is_static = true; }
            else if self.match_token(&Token::Async)  { is_async  = true; }
            else { break; }
        }
        (is_pub, is_static, is_async)
    }

    fn parse_stmt(&mut self) -> LunaResult<Stmt> {
        let tok = self.current().clone();
        let (line, col) = (tok.line, tok.col);

        let (is_pub, is_static, is_async) = self.parse_modifiers();

        match self.current().token.clone() {

            Token::Import => {
                self.advance();
                let name_tok = self.advance().clone();
                let module = match &name_tok.token {
                    Token::Identifier(n) => n.clone(),
                    other => return Err(LunaError::InvalidSyntax {
                        message: format!("expected module name, found `{other}`"),
                        line: name_tok.line, col: name_tok.col,
                    }),
                };
                let alias = if self.check(&Token::Identifier("as".to_string())) {
                    self.advance();
                    let a = self.advance().clone();
                    match &a.token {
                        Token::Identifier(n) => Some(n.clone()),
                        _ => None,
                    }
                } else { None };
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Import { module, alias, line, col })
            }

            Token::Let | Token::Const => {
                let is_const = matches!(self.current().token, Token::Const);
                self.advance();

                let mutable = !is_const && self.match_token(&Token::Mut);
                let name_tok = self.advance().clone();
                let name = match &name_tok.token {
                    Token::Identifier(n) => n.clone(),
                    other => return Err(LunaError::InvalidSyntax {
                        message: format!("expected variable name, found `{other}`"),
                        line: name_tok.line, col: name_tok.col,
                    }),
                };

                let ty = if self.check(&Token::Colon) {
                    self.advance();
                    self.parse_type()?
                } else {
                    LunaType::Inferred
                };

                let initializer = if self.check(&Token::Equals) {
                    self.advance();
                    Some(self.parse_expr()?)
                } else { None };

                self.expect(Token::Semicolon)?;

                if is_const {
                    let value = initializer.ok_or_else(|| LunaError::InvalidSyntax {
                        message: "const declarations require an initializer".to_string(),
                        line, col,
                    })?;
                    Ok(Stmt::ConstDecl { name, ty, value, line, col })
                } else {
                    Ok(Stmt::VarDecl { name, mutable, ty, initializer, is_pub, is_static, line, col })
                }
            }

            Token::Fn => {
                self.advance();
                let name_tok = self.advance().clone();
                let name = match &name_tok.token {
                    Token::Identifier(n) => n.clone(),
                    other => return Err(LunaError::InvalidSyntax {
                        message: format!("expected function name, found `{other}`"),
                        line: name_tok.line, col: name_tok.col,
                    }),
                };
                let params = self.parse_params()?;
                let return_type = if self.check(&Token::Arrow) {
                    self.advance();
                    self.parse_type()?
                } else {
                    LunaType::Void
                };
                let body = self.parse_block()?;
                Ok(Stmt::FnDecl { name, params, return_type, body, is_async, is_pub, is_static, line, col })
            }

            Token::Class => {
                self.advance();
                let name_tok = self.advance().clone();
                let name = match &name_tok.token {
                    Token::Identifier(n) => n.clone(),
                    other => return Err(LunaError::InvalidSyntax {
                        message: format!("expected class name, found `{other}`"),
                        line: name_tok.line, col: name_tok.col,
                    }),
                };
                let params = if self.check(&Token::LeftParen) {
                    self.parse_params()?
                } else { Vec::new() };
                let members = self.parse_block()?;
                Ok(Stmt::ClassDecl { name, params, members, is_pub, line, col })
            }

            Token::Namespace => {
                self.advance();
                let name_tok = self.advance().clone();
                let name = match &name_tok.token {
                    Token::Identifier(n) => n.clone(),
                    _ => return Err(LunaError::InvalidSyntax {
                        message: "expected namespace name".to_string(), line, col,
                    }),
                };
                let body = self.parse_block()?;
                Ok(Stmt::NamespaceDecl { name, body, line, col })
            }

            Token::Return => {
                self.advance();
                let value = if self.check(&Token::Semicolon) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Return { value, line, col })
            }

            Token::Break => {
                self.advance();
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Break { line, col })
            }

            Token::Continue => {
                self.advance();
                self.expect(Token::Semicolon)?;
                Ok(Stmt::Continue { line, col })
            }

            Token::If => {
                self.advance();
                self.expect(Token::LeftParen)?;
                let condition = self.parse_expr()?;
                self.expect(Token::RightParen)?;
                let then_body = self.parse_block()?;

                let mut else_if_branches = Vec::new();
                let mut else_body = None;

                while self.check(&Token::Else) {
                    self.advance();
                    if self.check(&Token::If) {
                        self.advance();
                        self.expect(Token::LeftParen)?;
                        let cond = self.parse_expr()?;
                        self.expect(Token::RightParen)?;
                        let body = self.parse_block()?;
                        else_if_branches.push((cond, body));
                    } else {
                        else_body = Some(self.parse_block()?);
                        break;
                    }
                }

                Ok(Stmt::If { condition, then_body, else_if_branches, else_body, line, col })
            }

            Token::While => {
                self.advance();
                self.expect(Token::LeftParen)?;
                let condition = self.parse_expr()?;
                self.expect(Token::RightParen)?;
                let body = self.parse_block()?;
                Ok(Stmt::While { condition, body, line, col })
            }

            Token::For => {
                self.advance();
                self.expect(Token::LeftParen)?;
                let var_tok = self.advance().clone();
                let variable = match &var_tok.token {
                    Token::Identifier(n) => n.clone(),
                    _ => return Err(LunaError::InvalidSyntax {
                        message: "expected variable name in for-in".to_string(), line, col,
                    }),
                };
                self.expect(Token::In)?;
                let iterable = self.parse_expr()?;
                self.expect(Token::RightParen)?;
                let body = self.parse_block()?;
                Ok(Stmt::For { variable, iterable, body, line, col })
            }

            _ => {
                let expr = self.parse_expr()?;
                self.expect(Token::Semicolon)?;
                Ok(Stmt::ExprStmt { expr, line, col })
            }
        }
    }

    pub fn parse(&mut self) -> LunaResult<Program> {
        let mut stmts = Vec::new();
        while !self.at_eof() {
            stmts.push(self.parse_stmt()?);
        }
        Ok(Program::new(stmts, self.source_path.clone()))
    }
}
