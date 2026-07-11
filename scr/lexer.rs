use super::token::{Token, TokenInfo};
use crate::errors::{LunaError, LunaResult};

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    #[inline]
    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    #[inline]
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    #[inline]
    fn peek2(&self) -> Option<char> {
        self.source.get(self.pos + 2).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn skip_whitespace_and_comments(&mut self) -> LunaResult<()> {
        loop {
            while matches!(self.current(), Some(c) if c.is_whitespace()) {
                self.advance();
            }

            if self.current() == Some('/') && self.peek() == Some('/') {
                while !matches!(self.current(), Some('\n') | None) {
                    self.advance();
                }
                continue;
            }

            if self.current() == Some('/') && self.peek() == Some('*') {
                let line = self.line;
                let col = self.col;
                self.advance();
                self.advance();
                let mut depth = 1usize;
                loop {
                    match (self.current(), self.peek()) {
                        (None, _) => {
                            return Err(LunaError::UnterminatedString { line, col });
                        }
                        (Some('/'), Some('*')) => {
                            self.advance();
                            self.advance();
                            depth += 1;
                        }
                        (Some('*'), Some('/')) => {
                            self.advance();
                            self.advance();
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
                continue;
            }

            break;
        }
        Ok(())
    }

    fn read_string(&mut self, start_line: usize, start_col: usize) -> LunaResult<Token> {
        let mut s = String::new();
        loop {
            match self.current() {
                None => {
                    return Err(LunaError::UnterminatedString {
                        line: start_line,
                        col: start_col,
                    })
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    let ch = match self.advance() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('"') => '"',
                        Some('\'') => '\'',
                        Some('\\') => '\\',
                        Some('0') => '\0',
                        Some(c) => {
                            return Err(LunaError::InvalidEscapeSequence {
                                ch: c,
                                line: start_line,
                                col: start_col,
                            })
                        }
                        None => {
                            return Err(LunaError::UnterminatedString {
                                line: start_line,
                                col: start_col,
                            })
                        }
                    };
                    s.push(ch);
                }
                Some(c) => {
                    s.push(c);
                    self.advance();
                }
            }
        }
        Ok(Token::StringLiteral(s))
    }

    fn read_fstring(&mut self, start_line: usize, start_col: usize) -> LunaResult<Token> {
        let mut s = String::new();
        loop {
            match self.current() {
                None => {
                    return Err(LunaError::UnterminatedString {
                        line: start_line,
                        col: start_col,
                    })
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some(c) => { s.push('\\'); s.push(c); }
                        None => {
                            return Err(LunaError::UnterminatedString {
                                line: start_line,
                                col: start_col,
                            })
                        }
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.advance();
                }
            }
        }
        Ok(Token::FStringLiteral(s))
    }

    fn read_number(&mut self) -> LunaResult<Token> {
        let mut s = String::new();
        let mut is_float = false;

        if self.current() == Some('0') && matches!(self.peek(), Some('x') | Some('X')) {
            s.push('0');
            self.advance();
            s.push(self.advance().unwrap());
            while let Some(c) = self.current() {
                if c.is_ascii_hexdigit() || c == '_' {
                    if c != '_' { s.push(c); }
                    self.advance();
                } else {
                    break;
                }
            }
            let n = u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0);
            return Ok(Token::NumberLiteral(n as f64));
        }

        while let Some(c) = self.current() {
            if c.is_ascii_digit() || c == '_' {
                if c != '_' { s.push(c); }
                self.advance();
            } else if c == '.' && !is_float && matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                is_float = true;
                s.push('.');
                self.advance();
            } else if (c == 'e' || c == 'E') && !s.is_empty() {
                s.push(c);
                self.advance();
                if matches!(self.current(), Some('+') | Some('-')) {
                    s.push(self.advance().unwrap());
                }
            } else {
                break;
            }
        }

        let n: f64 = s.parse().unwrap_or(0.0);
        Ok(Token::NumberLiteral(n))
    }

    fn read_identifier_or_keyword(&mut self) -> Token {
        let mut s = String::new();
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        keyword_or_identifier(s)
    }

    pub fn tokenize(&mut self) -> LunaResult<Vec<TokenInfo>> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace_and_comments()?;

            let line = self.line;
            let col = self.col;

            let ch = match self.current() {
                None => {
                    tokens.push(TokenInfo::new(Token::Eof, line, col, ""));
                    break;
                }
                Some(c) => c,
            };

            if ch == 'f' && self.peek() == Some('"') {
                self.advance();
                self.advance();
                let tok = self.read_fstring(line, col)?;
                let lexeme = tok.to_string();
                tokens.push(TokenInfo::new(tok, line, col, lexeme));
                continue;
            }

            if ch.is_alphabetic() || ch == '_' {
                let tok = self.read_identifier_or_keyword();
                let lexeme = tok.to_string();
                tokens.push(TokenInfo::new(tok, line, col, lexeme));
                continue;
            }

            if ch.is_ascii_digit() {
                let tok = self.read_number()?;
                let lexeme = tok.to_string();
                tokens.push(TokenInfo::new(tok, line, col, lexeme));
                continue;
            }

            if ch == '"' {
                self.advance();
                let tok = self.read_string(line, col)?;
                let lexeme = tok.to_string();
                tokens.push(TokenInfo::new(tok, line, col, lexeme));
                continue;
            }

            self.advance();
            let tok = match ch {
                '+' => match self.current() {
                    Some('=') => { self.advance(); Token::PlusEquals }
                    _ => Token::Plus,
                },
                '-' => match self.current() {
                    Some('>') => { self.advance(); Token::Arrow }
                    Some('=') => { self.advance(); Token::MinusEquals }
                    _ => Token::Minus,
                },
                '*' => match self.current() {
                    Some('*') => { self.advance(); Token::StarStar }
                    Some('=') => { self.advance(); Token::StarEquals }
                    _ => Token::Star,
                },
                '/' => match self.current() {
                    Some('=') => { self.advance(); Token::SlashEquals }
                    _ => Token::Slash,
                },
                '%' => Token::Percent,
                '=' => match self.current() {
                    Some('=') => { self.advance(); Token::EqualsEquals }
                    Some('>') => { self.advance(); Token::FatArrow }
                    _ => Token::Equals,
                },
                '!' => match self.current() {
                    Some('=') => { self.advance(); Token::BangEquals }
                    _ => Token::Bang,
                },
                '<' => match self.current() {
                    Some('=') => { self.advance(); Token::LessEquals }
                    Some('<') => { self.advance(); Token::ShiftLeft }
                    _ => Token::Less,
                },
                '>' => match self.current() {
                    Some('=') => { self.advance(); Token::GreaterEquals }
                    Some('>') => { self.advance(); Token::ShiftRight }
                    _ => Token::Greater,
                },
                '&' => match self.current() {
                    Some('&') => { self.advance(); Token::And }
                    _ => Token::Ampersand,
                },
                '|' => match self.current() {
                    Some('|') => { self.advance(); Token::Or }
                    _ => Token::Pipe,
                },
                '^' => Token::Caret,
                '~' => Token::Tilde,
                ':' => match self.current() {
                    Some(':') => { self.advance(); Token::ColonColon }
                    _ => Token::Colon,
                },
                '.' => match (self.current(), self.peek()) {
                    (Some('.'), Some('.')) => {
                        self.advance();
                        self.advance();
                        Token::DotDotDot
                    }
                    (Some('.'), _) => {
                        self.advance();
                        Token::DotDot
                    }
                    _ => Token::Dot,
                },
                '{' => Token::LeftBrace,
                '}' => Token::RightBrace,
                '(' => Token::LeftParen,
                ')' => Token::RightParen,
                '[' => Token::LeftBracket,
                ']' => Token::RightBracket,
                ';' => Token::Semicolon,
                ',' => Token::Comma,
                '?' => Token::QuestionMark,
                '@' => Token::At,
                '#' => Token::Hash,
                other => {
                    return Err(LunaError::UnexpectedChar { ch: other, line, col })
                }
            };

            let lexeme = tok.to_string();
            tokens.push(TokenInfo::new(tok, line, col, lexeme));
        }

        Ok(tokens)
    }
}

fn keyword_or_identifier(s: String) -> Token {
    match s.as_str() {
        "class"    => Token::Class,
        "fn"       => Token::Fn,
        "let"      => Token::Let,
        "mut"      => Token::Mut,
        "import"   => Token::Import,
        "return"   => Token::Return,
        "if"       => Token::If,
        "else"     => Token::Else,
        "while"    => Token::While,
        "for"      => Token::For,
        "in"       => Token::In,
        "break"    => Token::Break,
        "continue" => Token::Continue,
        "namespace"=> Token::Namespace,
        "async"    => Token::Async,
        "await"    => Token::Await,
        "pub"      => Token::Pub,
        "use"      => Token::Use,
        "mod"      => Token::Mod,
        "type"     => Token::Type,
        "enum"     => Token::Enum,
        "struct"   => Token::Struct,
        "impl"     => Token::Impl,
        "trait"    => Token::Trait,
        "match"    => Token::Match,
        "self"     => Token::Self_,
        "super"    => Token::Super,
        "new"      => Token::New,
        "delete"   => Token::Delete,
        "static"   => Token::Static,
        "const"    => Token::Const,
        "extern"   => Token::Extern,
        "true"     => Token::BoolLiteral(true),
        "false"    => Token::BoolLiteral(false),
        "null"     => Token::Null,
        "string"   => Token::TypeString,
        "number"   => Token::TypeNumber,
        "bool"     => Token::TypeBool,
        "void"     => Token::TypeVoid,
        "i32"      => Token::TypeI32,
        "i64"      => Token::TypeI64,
        "f32"      => Token::TypeF32,
        "f64"      => Token::TypeF64,
        "u8"       => Token::TypeU8,
        "usize"    => Token::TypeUsize,
        _          => Token::Identifier(s),
    }
}
