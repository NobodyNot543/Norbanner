//! The launcher's calculator: `>calc 2^10 / 4` or just `= sqrt(2) * pi`.
//!
//! A small recursive-descent parser over f64 with the usual precedence:
//! `+ -` < `* / %` < unary minus < `^` (right associative). Functions take
//! one argument; `pi`, `e` and `tau` are constants.

#[derive(Debug, PartialEq)]
pub enum CalcError {
    Syntax,
    UnknownName(String),
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    _src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars().collect(),
            pos: 0,
            _src: src,
        }
    }

    fn skip(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.chars.get(self.pos).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<f64, CalcError> {
        let mut value = self.term()?;
        loop {
            if self.eat('+') {
                value += self.term()?;
            } else if self.eat('-') {
                value -= self.term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64, CalcError> {
        let mut value = self.unary()?;
        loop {
            if self.eat('*') || self.eat('×') {
                value *= self.unary()?;
            } else if self.eat('/') || self.eat('÷') {
                value /= self.unary()?;
            } else if self.eat('%') {
                value %= self.unary()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn unary(&mut self) -> Result<f64, CalcError> {
        if self.eat('-') {
            return Ok(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<f64, CalcError> {
        let base = self.atom()?;
        if self.eat('^') {
            let exponent = self.unary()?;
            return Ok(base.powf(exponent));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<f64, CalcError> {
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let value = self.expr()?;
                if !self.eat(')') {
                    return Err(CalcError::Syntax);
                }
                Ok(value)
            }
            Some(c) if c.is_ascii_digit() || c == '.' || c == ',' => self.number(),
            Some(c) if c.is_alphabetic() => self.name(),
            _ => Err(CalcError::Syntax),
        }
    }

    fn number(&mut self) -> Result<f64, CalcError> {
        let start = self.pos;
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos]
            .iter()
            .map(|&c| if c == ',' { '.' } else { c })
            .collect();
        text.parse().map_err(|_| CalcError::Syntax)
    }

    fn name(&mut self) -> Result<f64, CalcError> {
        let start = self.pos;
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_alphanumeric())
        {
            self.pos += 1;
        }
        let name: String = self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let constant = match name.as_str() {
            "pi" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            "tau" => Some(std::f64::consts::TAU),
            _ => None,
        };
        if let Some(value) = constant {
            return Ok(value);
        }
        let function: fn(f64) -> f64 = match name.as_str() {
            "sqrt" => f64::sqrt,
            "cbrt" => f64::cbrt,
            "abs" => f64::abs,
            "sin" => f64::sin,
            "cos" => f64::cos,
            "tan" => f64::tan,
            "asin" => f64::asin,
            "acos" => f64::acos,
            "atan" => f64::atan,
            "ln" => f64::ln,
            "log" => f64::log10,
            "exp" => f64::exp,
            "floor" => f64::floor,
            "ceil" => f64::ceil,
            "round" => f64::round,
            _ => return Err(CalcError::UnknownName(name)),
        };
        let argument = self.atom()?;
        Ok(function(argument))
    }
}

/// Evaluates `src`.
pub fn eval(src: &str) -> Result<f64, CalcError> {
    let mut parser = Parser::new(src);
    let value = parser.expr()?;
    if parser.peek().is_some() {
        return Err(CalcError::Syntax);
    }
    Ok(value)
}

/// `value` as the launcher shows it: up to 10 significant decimals,
/// without trailing zeros.
#[must_use]
pub fn format(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{value:.0}");
    }
    let text = format!("{value:.10}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Whether `text` looks like arithmetic worth showing a live result for.
#[must_use]
pub fn looks_like_math(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| "+-*/^%()×÷".contains(c))
        && eval(t).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence() {
        assert_eq!(eval("2 + 3 * 4"), Ok(14.0));
        assert_eq!(eval("(2 + 3) * 4"), Ok(20.0));
        assert_eq!(eval("2 ^ 3 ^ 2"), Ok(512.0));
        assert_eq!(eval("-2 ^ 2"), Ok(-4.0));
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(eval("sqrt(16)"), Ok(4.0));
        assert!((eval("cos(pi)").unwrap_or_default() + 1.0).abs() < 1e-12);
        assert!(matches!(eval("foo(2)"), Err(CalcError::UnknownName(_))));
    }

    #[test]
    fn formatting() {
        assert_eq!(format(4.0), "4");
        assert_eq!(format(0.1 + 0.2), "0.3");
        assert!(looks_like_math("2+2"));
        assert!(!looks_like_math("firefox"));
    }
}
