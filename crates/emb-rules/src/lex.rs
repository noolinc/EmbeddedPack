//! A small C/C++ lexer: enough structure to check rules without false hits
//! inside comments and string literals. Linear in the input, no recursion,
//! no floats, never panics on malformed input.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The `#` that opens a preprocessor directive (not the `#` operator).
    Hash,
    Ident,
    Number,
    Str,
    Char,
    Punct,
}

#[derive(Clone, Copy, Debug)]
pub struct Tok<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub line: u32,
    /// Inside a preprocessor directive.
    pub pp: bool,
}

impl<'a> Tok<'a> {
    pub fn is(&self, s: &str) -> bool {
        self.kind != Kind::Str && self.kind != Kind::Char && self.text == s
    }
    pub fn ident(&self) -> Option<&'a str> {
        (self.kind == Kind::Ident).then_some(self.text)
    }
    /// A string literal's body, without the quotes or any prefix.
    pub fn str_body(&self) -> Option<&'a str> {
        if self.kind != Kind::Str {
            return None;
        }
        let t = self.text;
        let a = t.find(['"', '<'])? + 1;
        let b = t.rfind(['"', '>'])?;
        (a <= b).then(|| &t[a..b])
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Comment<'a> {
    pub text: &'a str,
    pub line: u32,
    pub end_line: u32,
    pub block: bool,
}

#[derive(Default, Debug)]
pub struct Lexed<'a> {
    pub toks: Vec<Tok<'a>>,
    pub comments: Vec<Comment<'a>>,
    /// For each token, the index of its matching bracket, or `u32::MAX`.
    pub pair: Vec<u32>,
}

pub const NONE: u32 = u32::MAX;

const P3: [&str; 5] = ["<<=", ">>=", "...", "->*", "<=>"];
const P2: [&str; 21] = [
    "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=", "*=", "/=", "%=",
    "&=", "|=", "^=", "::", "##",
];

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80
}
fn is_ident_part(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

pub fn lex(src: &str) -> Lexed<'_> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Lexed::default();
    let (mut i, mut line) = (0usize, 1u32);
    let mut pp = false;
    let mut line_start = true;
    let mut include = false;

    while i < n {
        let c = b[i];
        if c == b'\\' && i + 1 < n && (b[i + 1] == b'\n' || (b[i + 1] == b'\r' && i + 2 < n)) {
            i += if b[i + 1] == b'\r' { 3 } else { 2 };
            line += 1;
            continue;
        }
        if c == b'\n' {
            line += 1;
            pp = false;
            include = false;
            line_start = true;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let s = i;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            out.comments.push(Comment { text: &src[s..i], line, end_line: line, block: false });
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let (s, l0) = (i, line);
            i += 2;
            while i < n && !(b[i] == b'*' && i + 1 < n && b[i + 1] == b'/') {
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(n);
            out.comments.push(Comment { text: &src[s..i], line: l0, end_line: line, block: true });
            continue;
        }
        if c == b'#' && line_start {
            pp = true;
            line_start = false;
            out.toks.push(Tok { kind: Kind::Hash, text: &src[i..i + 1], line, pp });
            i += 1;
            // Directive name decides whether `<...>` is a header name.
            let mut j = i;
            while j < n && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            include = src[j..].starts_with("include");
            continue;
        }
        line_start = false;
        if c == b'"' || (c == b'<' && pp && include) {
            let close = if c == b'"' { b'"' } else { b'>' };
            let s = i;
            // Raw string: the identifier just lexed ends in `R` and touches the quote.
            let raw = c == b'"'
                && out.toks.last().is_some_and(|t| {
                    t.kind == Kind::Ident
                        && t.text.ends_with('R')
                        && t.text.len() <= 3
                        && t.text.as_ptr() as usize + t.text.len() == src.as_ptr() as usize + s
                });
            i += 1;
            if raw {
                let d0 = i;
                while i < n && b[i] != b'(' && b[i] != b'\n' {
                    i += 1;
                }
                let delim = &src[d0..i.min(n)];
                loop {
                    if i >= n {
                        break;
                    }
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    if b[i] == b')' && src[i + 1..].starts_with(delim) {
                        let e = i + 1 + delim.len();
                        if e < n && b[e] == b'"' {
                            i = e + 1;
                            break;
                        }
                    }
                    i += 1;
                }
            } else {
                while i < n && b[i] != close && b[i] != b'\n' {
                    if b[i] == b'\\' && i + 1 < n {
                        if b[i + 1] == b'\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                    i += 1;
                }
                if i < n && b[i] == close {
                    i += 1;
                }
            }
            let i2 = i.min(n);
            out.toks.push(Tok { kind: Kind::Str, text: &src[s..i2], line, pp });
            i = i2;
            continue;
        }
        if c == b'\'' {
            let s = i;
            i += 1;
            while i < n && b[i] != b'\'' && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            out.toks.push(Tok { kind: Kind::Char, text: &src[s..i], line, pp });
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && i + 1 < n && b[i + 1].is_ascii_digit()) {
            let s = i;
            let hex = c == b'0' && i + 1 < n && (b[i + 1] | 0x20) == b'x';
            i += 1;
            while i < n {
                let d = b[i];
                let prev = b[i - 1] | 0x20;
                if is_ident_part(d) || d == b'.' {
                    i += 1;
                } else if d == b'\'' && i + 1 < n && b[i + 1].is_ascii_alphanumeric() {
                    i += 2;
                } else if (d == b'+' || d == b'-') && ((prev == b'e' && !hex) || prev == b'p') {
                    i += 1;
                } else {
                    break;
                }
            }
            out.toks.push(Tok { kind: Kind::Number, text: &src[s..i], line, pp });
            continue;
        }
        if is_ident_start(c) {
            let s = i;
            while i < n && is_ident_part(b[i]) {
                i += 1;
            }
            out.toks.push(Tok { kind: Kind::Ident, text: &src[s..i], line, pp });
            continue;
        }
        let rest = &src[i..];
        let len = if P3.iter().any(|p| rest.starts_with(p)) {
            3
        } else if P2.iter().any(|p| rest.starts_with(p)) {
            2
        } else {
            // Step over one whole character, whatever its width.
            rest.chars().next().map_or(1, char::len_utf8)
        };
        out.toks.push(Tok { kind: Kind::Punct, text: &src[i..i + len], line, pp });
        i += len;
    }

    // Bracket matching over code tokens only: a macro body may be unbalanced.
    out.pair = vec![NONE; out.toks.len()];
    let mut stack: Vec<u32> = Vec::new();
    for (k, t) in out.toks.iter().enumerate() {
        if t.pp || t.kind != Kind::Punct {
            continue;
        }
        match t.text {
            "(" | "[" | "{" => stack.push(k as u32),
            ")" | "]" | "}" => {
                let want = match t.text {
                    ")" => "(",
                    "]" => "[",
                    _ => "{",
                };
                // Pop to the nearest opener of the right shape; `#if` branches
                // can leave strays behind.
                if let Some(pos) = stack.iter().rposition(|&o| out.toks[o as usize].text == want) {
                    let o = stack[pos];
                    stack.truncate(pos);
                    out.pair[o as usize] = k as u32;
                    out.pair[k] = o;
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<&str> {
        lex(src).toks.iter().map(|t| t.text).collect()
    }

    #[test]
    fn comments_and_strings_are_not_code() {
        let l = lex("int a; // goto x\n/* malloc( */ char *s = \"goto \\\" y\";");
        assert!(!l.toks.iter().any(|t| t.is("goto") || t.is("malloc")));
        assert_eq!(l.comments.len(), 2);
        assert_eq!(l.toks.last().unwrap().text, ";");
    }

    #[test]
    fn numbers_keep_exponents_and_separators() {
        assert_eq!(texts("x = 1e-3 + 0x1F - 1'000 + 0xE+1;"), ["x", "=", "1e-3", "+", "0x1F", "-", "1'000", "+", "0xE", "+", "1", ";"]);
    }

    #[test]
    fn preprocessor_lines_are_flagged_and_headers_are_strings() {
        let l = lex("#include <stdio.h>\n#define A(x) \\\n  (x)\nint b;");
        assert_eq!(l.toks[2].kind, Kind::Str);
        assert_eq!(l.toks[2].str_body(), Some("stdio.h"));
        assert!(l.toks.iter().filter(|t| t.pp).count() >= 9);
        let b = l.toks.iter().find(|t| t.is("b")).unwrap();
        assert!(!b.pp);
        assert_eq!(b.line, 4);
    }

    #[test]
    fn brackets_pair_up_and_survive_garbage() {
        let l = lex("f(a[1]) { } ) ] }");
        assert_eq!(l.pair[1], 6);
        assert_eq!(l.pair[3], 5);
        assert_eq!(l.pair[9], NONE);
        let _ = lex("\"unterminated\n'x /* open");
        let _ = lex("R\"x(raw \" body)x\" é ∑ #");
    }

    #[test]
    fn raw_strings_are_one_token() {
        let l = lex("auto s = R\"d(goto \" x)d\"; int y;");
        assert!(!l.toks.iter().any(|t| t.is("goto")));
        assert!(l.toks.iter().any(|t| t.is("y")));
    }
}
