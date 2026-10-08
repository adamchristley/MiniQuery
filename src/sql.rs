//! A deliberately small SQL grammar. Unsupported syntax is rejected explicitly.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub table: Option<String>,
    pub name: String,
}

impl Column {
    pub fn qualified(&self) -> String {
        match &self.table {
            Some(table) => format!("{table}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Integer(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Column(Column),
    Literal(Literal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    pub left: Column,
    pub op: Operator,
    pub right: Operand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    pub table: String,
    pub left: Column,
    pub right: Column,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub select_all: bool,
    pub projection: Vec<Column>,
    pub from: String,
    pub join: Option<Join>,
    pub filters: Vec<Predicate>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Integer(i64),
    Text(String),
    Symbol(char),
    Operator(String),
}

fn tokenize(sql: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut chars = sql.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut word = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    word.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Token::Word(word));
        } else if c.is_ascii_digit() || c == '-' {
            let mut digits = String::new();
            if c == '-' {
                digits.push(c);
                chars.next();
                if !chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                    return Err("expected integer after '-'".into());
                }
            }
            while let Some(&n) = chars.peek() {
                if n.is_ascii_digit() {
                    digits.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Token::Integer(digits.parse::<i64>().map_err(|e| e.to_string())?));
        } else if c == '\'' {
            chars.next();
            let mut value = String::new();
            let mut closed = false;
            while let Some(n) = chars.next() {
                if n == '\'' {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                        value.push('\'');
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    value.push(n);
                }
            }
            if !closed {
                return Err("unterminated SQL string literal".into());
            }
            out.push(Token::Text(value));
        } else if matches!(c, ',' | '.' | '*' | ';' | '(' | ')') {
            chars.next();
            out.push(Token::Symbol(c));
        } else if matches!(c, '=' | '!' | '<' | '>') {
            chars.next();
            let mut op = c.to_string();
            if chars.peek() == Some(&'=') {
                op.push('=');
                chars.next();
            } else if c == '<' && chars.peek() == Some(&'>') {
                op.push('>');
                chars.next();
            }
            out.push(Token::Operator(op));
        } else {
            return Err(format!("unsupported SQL character: {c:?}"));
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.peek().cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn keyword(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w.eq_ignore_ascii_case(word))
    }

    fn eat_keyword(&mut self, word: &str) -> bool {
        if self.keyword(word) {
            self.take();
            true
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, word: &str) -> Result<(), String> {
        if self.eat_keyword(word) {
            Ok(())
        } else {
            Err(format!("expected keyword {word} near {:?}", self.peek()))
        }
    }

    fn eat_symbol(&mut self, symbol: char) -> bool {
        if self.peek() == Some(&Token::Symbol(symbol)) {
            self.take();
            true
        } else {
            false
        }
    }

    fn identifier(&mut self) -> Result<String, String> {
        match self.take() {
            Some(Token::Word(word)) => Ok(word),
            other => Err(format!("expected identifier, got {other:?}")),
        }
    }

    fn column(&mut self) -> Result<Column, String> {
        let first = self.identifier()?;
        if self.eat_symbol('.') {
            Ok(Column {
                table: Some(first),
                name: self.identifier()?,
            })
        } else {
            Ok(Column {
                table: None,
                name: first,
            })
        }
    }

    fn predicate(&mut self) -> Result<Predicate, String> {
        let left = self.column()?;
        let op = match self.take() {
            Some(Token::Operator(s)) => match s.as_str() {
                "=" => Operator::Eq,
                "!=" | "<>" => Operator::Ne,
                "<" => Operator::Lt,
                "<=" => Operator::Le,
                ">" => Operator::Gt,
                ">=" => Operator::Ge,
                _ => return Err(format!("unsupported comparison operator {s}")),
            },
            other => return Err(format!("expected comparison operator, got {other:?}")),
        };
        let right = match self.peek() {
            Some(Token::Integer(_)) => match self.take() {
                Some(Token::Integer(i)) => Operand::Literal(Literal::Integer(i)),
                _ => unreachable!(),
            },
            Some(Token::Text(_)) => match self.take() {
                Some(Token::Text(s)) => Operand::Literal(Literal::Text(s)),
                _ => unreachable!(),
            },
            _ => Operand::Column(self.column()?),
        };
        Ok(Predicate { left, op, right })
    }
}

pub fn parse(sql: &str) -> Result<Query, String> {
    let mut p = Parser {
        tokens: tokenize(sql)?,
        pos: 0,
    };
    p.expect_keyword("SELECT")?;
    let select_all = p.eat_symbol('*');
    let mut projection = Vec::new();
    if !select_all {
        projection.push(p.column()?);
        while p.eat_symbol(',') {
            projection.push(p.column()?);
        }
    }
    p.expect_keyword("FROM")?;
    let from = p.identifier()?;

    let join = if p.eat_keyword("INNER") {
        p.expect_keyword("JOIN")?;
        Some(parse_join(&mut p)?)
    } else if p.eat_keyword("JOIN") {
        Some(parse_join(&mut p)?)
    } else {
        None
    };

    let mut filters = Vec::new();
    if p.eat_keyword("WHERE") {
        filters.push(p.predicate()?);
        while p.eat_keyword("AND") {
            filters.push(p.predicate()?);
        }
    }

    let limit = if p.eat_keyword("LIMIT") {
        match p.take() {
            Some(Token::Integer(n)) if n >= 0 => Some(n as usize),
            other => return Err(format!("LIMIT requires non-negative integer, got {other:?}")),
        }
    } else {
        None
    };
    p.eat_symbol(';');
    if p.peek().is_some() {
        return Err(format!("unexpected SQL input: {:?}", p.peek()));
    }
    Ok(Query {
        select_all,
        projection,
        from,
        join,
        filters,
        limit,
    })
}

fn parse_join(p: &mut Parser) -> Result<Join, String> {
    let table = p.identifier()?;
    p.expect_keyword("ON")?;
    let left = p.column()?;
    if p.take() != Some(Token::Operator("=".into())) {
        return Err("JOIN ON currently supports equality only".into());
    }
    let right = p.column()?;
    Ok(Join {
        table,
        left,
        right,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_join_filter_and_limit() {
        let q = parse("select customers.name FROM customers INNER JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 50 AND customers.name != 'Bob' LIMIT 3;").unwrap();
        assert_eq!(q.projection[0].qualified(), "customers.name");
        assert_eq!(q.join.unwrap().table, "orders");
        assert_eq!(q.filters.len(), 2);
        assert_eq!(q.limit, Some(3));
    }

    #[test]
    fn handles_escaped_quotes_and_bad_syntax() {
        let q = parse("SELECT name FROM customers WHERE name = 'O''Brien'").unwrap();
        assert_eq!(q.filters[0].right, Operand::Literal(Literal::Text("O'Brien".into())));
        assert!(parse("SELECT FROM customers").is_err());
        assert!(parse("SELECT x FROM t WHERE x = 'unfinished").is_err());
        assert!(parse("SELECT x FROM t LIMIT -1").is_err());
        assert!(parse("SELECT x FROM t ORDER BY x").is_err());
    }
}
