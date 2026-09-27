//! TeX math as Unicode: [`inline`] for `$…$` in running text, [`display`] for
//! `$$…$$` blocks, which stack fractions, roots, limits, and matrices over
//! several rows. Unknown commands stay as written so they are easy to spot.

use unicode_width::UnicodeWidthStr;

/// How a symbol spaces against its neighbors (TeX's atom classes).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

#[derive(Debug)]
enum Node {
    Sym(String, Class),
    /// Explicit space (`\,`, `\quad`), in columns.
    Space(usize),
    Row(Vec<Node>),
    /// Numerator and denominator; no bar for `\binom`.
    Frac(Box<Node>, Box<Node>, bool),
    Sqrt(Option<Box<Node>>, Box<Node>),
    Scripts {
        base: Box<Node>,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    Fenced(String, Box<Node>, String),
    /// Environments (`pmatrix`, `cases`, `aligned`) and lines split by `\\`.
    Grid {
        rows: Vec<Vec<Node>>,
        align: Align,
        open: &'static str,
        close: &'static str,
    },
}

#[derive(Clone, Copy, Debug)]
enum Align {
    Center,
    Left,
    /// `aligned`: columns pair up around the `&`, right then left.
    RightLeft,
}

/// `tex` on one line, for inline math.
pub fn inline(tex: &str) -> String {
    linear(&Parser::new(tex).parse(), false)
}

/// `tex` laid out over as many rows as it needs, sharing a left edge.
pub fn display(tex: &str) -> Vec<String> {
    if tex.trim().is_empty() {
        return Vec::new();
    }
    layout(&Parser::new(tex).parse())
        .rows
        .into_iter()
        .map(|r| r.trim_end().to_string())
        .collect()
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(tex: &str) -> Self {
        Self {
            chars: tex.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    /// Whether `\name` (not a longer command) comes next.
    fn at_command(&self, name: &str) -> bool {
        let rest = &self.chars[self.pos.min(self.chars.len())..];
        let n = name.chars().count();
        rest.first() == Some(&'\\')
            && rest.iter().skip(1).take(n).copied().eq(name.chars())
            && !(name.starts_with(char::is_alphabetic)
                && rest.get(n + 1).is_some_and(|c| c.is_ascii_alphabetic()))
    }

    fn eat_command(&mut self, name: &str) -> bool {
        let found = self.at_command(name);
        if found {
            self.pos += 1 + name.chars().count();
        }
        found
    }

    /// The whole input, split into lines and cells if it has `\\` or `&`.
    fn parse(&mut self) -> Node {
        let mut rows = self.grid(false);
        if rows.len() == 1 && rows[0].len() == 1 {
            return rows.remove(0).remove(0);
        }
        let align = if rows.iter().any(|r| r.len() > 1) {
            Align::RightLeft
        } else {
            Align::Center
        };
        Node::Grid {
            rows,
            align,
            open: "",
            close: "",
        }
    }

    /// Rows of `&`-separated cells up to `\end{…}` (in an environment) or the
    /// end of input.
    fn grid(&mut self, in_env: bool) -> Vec<Vec<Node>> {
        let mut rows = Vec::new();
        let mut cells = Vec::new();
        loop {
            let mut items = self.row();
            while self.skip_stray(in_env) {
                items.extend(self.row());
            }
            cells.push(Node::Row(items));
            if self.peek() == Some('&') {
                self.pos += 1;
            } else if self.eat_command("\\") {
                rows.push(std::mem::take(&mut cells));
            } else {
                if self.eat_command("end") {
                    self.raw_group();
                }
                break;
            }
        }
        if !cells
            .iter()
            .all(|c| matches!(c, Node::Row(r) if r.is_empty()))
            || rows.is_empty()
        {
            rows.push(cells);
        }
        rows
    }

    /// Skips a `}`, `\right`, or (outside an environment) `\end` with nothing
    /// to close.
    fn skip_stray(&mut self, in_env: bool) -> bool {
        self.skip_spaces();
        if self.peek() == Some('}') {
            self.pos += 1;
        } else if self.eat_command("right") {
            self.delimiter();
        } else if !in_env && self.eat_command("end") {
            self.raw_group();
        } else {
            return false;
        }
        true
    }

    /// Items up to the end of the group, a cell, or a line.
    fn row(&mut self) -> Vec<Node> {
        let mut items: Vec<Node> = Vec::new();
        loop {
            self.skip_spaces();
            let Some(c) = self.peek() else { break };
            if c == '}'
                || c == '&'
                || self.at_command("\\")
                || self.at_command("right")
                || self.at_command("end")
            {
                break;
            }
            match c {
                '^' | '_' => {
                    self.pos += 1;
                    let arg = Box::new(self.arg());
                    let base = items.pop().unwrap_or(Node::Sym(String::new(), Class::Ord));
                    let (base, mut sub, mut sup) = match base {
                        Node::Scripts { base, sub, sup } => (base, sub, sup),
                        other => (Box::new(other), None, None),
                    };
                    let slot = if c == '^' { &mut sup } else { &mut sub };
                    match slot {
                        // `x^a^b` is an error in TeX; keep both rather than drop one.
                        Some(prev) => {
                            let prev = std::mem::replace(prev, Box::new(Node::Row(Vec::new())));
                            *slot = Some(Box::new(Node::Row(vec![*prev, *arg])));
                        }
                        None => *slot = Some(arg),
                    }
                    items.push(Node::Scripts { base, sub, sup });
                }
                '\'' => {
                    self.pos += 1;
                    items.push(Node::Sym("′".into(), Class::Ord));
                }
                _ => {
                    if let Some(node) = self.atom() {
                        let closes = matches!(&node, Node::Sym(_, Class::Close));
                        items.push(node);
                        if closes {
                            fold_parens(&mut items);
                        }
                    }
                }
            }
        }
        items
    }

    /// A `{…}` group or a single symbol, as a command argument or script.
    fn arg(&mut self) -> Node {
        self.skip_spaces();
        self.atom().unwrap_or(Node::Row(Vec::new()))
    }

    fn atom(&mut self) -> Option<Node> {
        let c = self.peek()?;
        self.pos += 1;
        Some(match c {
            '{' => {
                let row = self.row();
                if self.peek() == Some('}') {
                    self.pos += 1;
                }
                Node::Row(row)
            }
            '\\' => return self.command(),
            '~' => Node::Space(1),
            c => char_symbol(c),
        })
    }

    /// The text of a `{…}` group, braces balanced, or the next character.
    fn raw_group(&mut self) -> String {
        self.skip_spaces();
        if self.peek() != Some('{') {
            return self.atom_text();
        }
        self.pos += 1;
        let mut depth = 0;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => break,
                '}' => depth -= 1,
                _ => {}
            }
            out.push(c);
        }
        out
    }

    fn atom_text(&mut self) -> String {
        let c = self.peek().map(String::from).unwrap_or_default();
        self.pos += usize::from(!c.is_empty());
        c
    }

    fn delimiter(&mut self) -> String {
        self.skip_spaces();
        match self.peek() {
            Some('\\') => {
                self.pos += 1;
                let name = self.name();
                match name.as_str() {
                    "{" | "lbrace" => "{",
                    "}" | "rbrace" => "}",
                    "|" | "Vert" | "lVert" | "rVert" => "‖",
                    "vert" | "lvert" | "rvert" => "|",
                    "langle" => "⟨",
                    "rangle" => "⟩",
                    "lfloor" => "⌊",
                    "rfloor" => "⌋",
                    "lceil" => "⌈",
                    "rceil" => "⌉",
                    _ => "",
                }
                .to_string()
            }
            Some('.') => {
                self.pos += 1;
                String::new()
            }
            _ => self.atom_text(),
        }
    }

    /// A command name after `\`: letters, or one other character.
    fn name(&mut self) -> String {
        let letters: String = self.chars[self.pos..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        if letters.is_empty() {
            self.atom_text()
        } else {
            self.pos += letters.len();
            letters
        }
    }

    fn command(&mut self) -> Option<Node> {
        let name = self.name();
        let node = match name.as_str() {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                Node::Frac(Box::new(self.arg()), Box::new(self.arg()), true)
            }
            "binom" | "dbinom" | "tbinom" => {
                Node::Frac(Box::new(self.arg()), Box::new(self.arg()), false)
            }
            "sqrt" => {
                self.skip_spaces();
                let index = (self.peek() == Some('[')).then(|| {
                    let start = self.pos + 1;
                    let end = self.chars[start..]
                        .iter()
                        .position(|&c| c == ']')
                        .map_or(self.chars.len(), |i| start + i);
                    self.pos = (end + 1).min(self.chars.len());
                    let index: String = self.chars[start..end].iter().collect();
                    Box::new(Parser::new(&index).parse())
                });
                Node::Sqrt(index, Box::new(self.arg()))
            }
            "text" | "textrm" | "textnormal" | "textit" | "textbf" | "textsf" | "texttt"
            | "mbox" | "mathrm" | "mathit" | "mathsf" | "mathtt" => {
                Node::Sym(self.raw_group(), Class::Ord)
            }
            "operatorname" => Node::Sym(self.raw_group(), Class::Op),
            "mathbb" | "mathcal" | "mathscr" | "mathfrak" | "mathbf" | "boldsymbol" | "bm" => {
                let text = self.raw_group();
                Node::Sym(
                    text.chars().map(|c| alphabet(&name, c)).collect(),
                    Class::Ord,
                )
            }
            "hat" | "widehat" | "bar" | "overline" | "vec" | "overrightarrow" | "dot" | "ddot"
            | "tilde" | "widetilde" | "underline" => {
                let mark = match name.as_str() {
                    "hat" | "widehat" => '\u{302}',
                    "bar" | "overline" => '\u{305}',
                    "vec" | "overrightarrow" => '\u{20d7}',
                    "dot" => '\u{307}',
                    "ddot" => '\u{308}',
                    "underline" => '\u{332}',
                    _ => '\u{303}',
                };
                let text = linear(&self.arg(), true);
                Node::Sym(text.chars().flat_map(|c| [c, mark]).collect(), Class::Ord)
            }
            "left" => {
                let open = self.delimiter();
                let body = self.row();
                let close = if self.eat_command("right") {
                    self.delimiter()
                } else {
                    String::new()
                };
                Node::Fenced(open, Box::new(Node::Row(body)), close)
            }
            "begin" => {
                let env = self.raw_group();
                let env = env.trim_end_matches('*');
                if env == "array" {
                    self.raw_group();
                }
                let (open, close, align) = match env {
                    "matrix" | "smallmatrix" => ("", "", Align::Center),
                    "pmatrix" => ("(", ")", Align::Center),
                    "bmatrix" => ("[", "]", Align::Center),
                    "Bmatrix" => ("{", "}", Align::Center),
                    "vmatrix" => ("|", "|", Align::Center),
                    "Vmatrix" => ("‖", "‖", Align::Center),
                    "cases" => ("{", "", Align::Left),
                    "aligned" | "align" | "alignat" | "split" | "eqnarray" => {
                        ("", "", Align::RightLeft)
                    }
                    _ => ("", "", Align::Center),
                };
                let rows = self.grid(true);
                Node::Grid {
                    rows,
                    align,
                    open,
                    close,
                }
            }
            "not" => {
                let Some(Node::Sym(s, class)) = self.atom() else {
                    return None;
                };
                let negated = match s.as_str() {
                    "=" => "≠".to_string(),
                    "∈" => "∉".to_string(),
                    "⊂" => "⊄".to_string(),
                    "⊆" => "⊈".to_string(),
                    "≡" => "≢".to_string(),
                    _ => format!("{s}\u{338}"),
                };
                Node::Sym(negated, class)
            }
            "pmod" => {
                let n = linear(&self.arg(), false);
                Node::Row(vec![
                    Node::Space(1),
                    Node::Sym(format!("(mod {n})"), Class::Ord),
                ])
            }
            "," | ":" | ";" | ">" | " " => Node::Space(1),
            "quad" => Node::Space(2),
            "qquad" => Node::Space(4),
            "!" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl" | "Bigr" | "biggl"
            | "biggr" | "Biggl" | "Biggr" | "displaystyle" | "textstyle" | "limits"
            | "nolimits" | "nonumber" | "notag" => return None,
            "liminf" => Node::Sym("lim inf".into(), Class::Op),
            "limsup" => Node::Sym("lim sup".into(), Class::Op),
            other => match symbol(other) {
                Some((s, class)) => Node::Sym(s.to_string(), class),
                None if FUNCTIONS.contains(&other) => Node::Sym(other.to_string(), Class::Op),
                // Spaced like an operator name so `\foo x` keeps its space.
                None => Node::Sym(format!("\\{other}"), Class::Op),
            },
        };
        Some(node)
    }
}

/// Upright operator names, spaced from what follows.
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "log", "ln", "lg", "exp", "det", "dim", "ker", "deg", "gcd", "lcm", "hom", "arg", "Pr",
    "lim", "max", "min", "sup", "inf",
];

/// Operators whose scripts go under and over them in display math.
fn takes_limits(node: &Node) -> bool {
    matches!(node, Node::Sym(s, Class::Op) if matches!(
        s.as_str(),
        "∑" | "∏" | "∐" | "⋃" | "⋂" | "⨁" | "⨂" | "⋁" | "⋀"
            | "lim" | "lim inf" | "lim sup" | "max" | "min" | "sup" | "inf" | "det" | "gcd" | "Pr"
    ))
}

/// Folds the items since the `(` or `[` matching the `)` or `]` just pushed
/// into one fenced node, so a script after it applies to the whole group and
/// display math can grow the brackets around tall content.
fn fold_parens(items: &mut Vec<Node>) {
    let Some(Node::Sym(close, _)) = items.last() else {
        return;
    };
    let open = match close.as_str() {
        ")" => "(",
        "]" => "[",
        "}" => "{",
        _ => return,
    };
    let Some(start) = items
        .iter()
        .rposition(|n| matches!(n, Node::Sym(s, Class::Open) if s == open))
    else {
        return;
    };
    let Some(Node::Sym(close, _)) = items.pop() else {
        return;
    };
    let inner: Vec<Node> = items.drain(start + 1..).collect();
    items.pop();
    items.push(Node::Fenced(
        open.to_string(),
        Box::new(Node::Row(inner)),
        close,
    ));
}

fn char_symbol(c: char) -> Node {
    let (s, class) = match c {
        '+' => ("+", Class::Bin),
        '-' => ("−", Class::Bin),
        '*' => ("∗", Class::Bin),
        '=' | '<' | '>' | ':' => return Node::Sym(c.to_string(), Class::Rel),
        ',' | ';' => return Node::Sym(c.to_string(), Class::Punct),
        '(' | '[' => return Node::Sym(c.to_string(), Class::Open),
        ')' | ']' => return Node::Sym(c.to_string(), Class::Close),
        c => return Node::Sym(c.to_string(), Class::Ord),
    };
    Node::Sym(s.to_string(), class)
}

/// Whitespace-separated `name glyph` pairs for each class.
const SYMBOLS: &[(Class, &str)] = &[
    (
        Class::Ord,
        "alpha α beta β gamma γ delta δ epsilon ϵ varepsilon ε zeta ζ eta η theta θ vartheta ϑ \
         iota ι kappa κ lambda λ mu μ nu ν xi ξ pi π varpi ϖ rho ρ varrho ϱ sigma σ varsigma ς \
         tau τ upsilon υ phi ϕ varphi φ chi χ psi ψ omega ω Gamma Γ Delta Δ Theta Θ Lambda Λ \
         Xi Ξ Pi Π Sigma Σ Upsilon Υ Phi Φ Psi Ψ Omega Ω infty ∞ partial ∂ nabla ∇ hbar ℏ ell ℓ \
         Re ℜ Im ℑ aleph ℵ emptyset ∅ varnothing ∅ forall ∀ exists ∃ nexists ∄ neg ¬ lnot ¬ \
         angle ∠ triangle △ degree ° prime ′ dagger † ddagger ‡ ldots … dots … dotsc … \
         cdots ⋯ dotsb ⋯ vdots ⋮ ddots ⋱ | ‖ % % $ $ & & # # _ _",
    ),
    (Class::Open, "{ { lbrace { langle ⟨ lceil ⌈ lfloor ⌊"),
    (Class::Close, "} } rbrace } rangle ⟩ rceil ⌉ rfloor ⌋"),
    (
        Class::Op,
        "sum ∑ prod ∏ coprod ∐ int ∫ iint ∬ iiint ∭ oint ∮ bigcup ⋃ bigcap ⋂ bigoplus ⨁ \
         bigotimes ⨂ bigvee ⋁ bigwedge ⋀",
    ),
    (
        Class::Bin,
        "cdot ⋅ times × div ÷ pm ± mp ∓ circ ∘ bullet ∙ star ⋆ ast ∗ cup ∪ cap ∩ setminus ∖ \
         oplus ⊕ ominus ⊖ otimes ⊗ odot ⊙ wedge ∧ land ∧ vee ∨ lor ∨ mod mod bmod mod",
    ),
    (
        Class::Rel,
        "leq ≤ le ≤ geq ≥ ge ≥ neq ≠ ne ≠ ll ≪ gg ≫ approx ≈ equiv ≡ sim ∼ simeq ≃ cong ≅ \
         propto ∝ in ∈ notin ∉ ni ∋ subset ⊂ subseteq ⊆ supset ⊃ supseteq ⊇ perp ⊥ \
         parallel ∥ mid ∣ to → rightarrow → gets ← leftarrow ← leftrightarrow ↔ Rightarrow ⇒ \
         Leftarrow ⇐ Leftrightarrow ⇔ implies ⟹ impliedby ⟸ iff ⟺ mapsto ↦ uparrow ↑ \
         downarrow ↓ coloneqq ≔ vdash ⊢ models ⊨",
    ),
];

/// The value for `key` in a table of whitespace-separated pairs.
fn lookup<'a>(table: &'a str, key: &str) -> Option<&'a str> {
    let mut words = table.split_whitespace();
    while let (Some(k), Some(v)) = (words.next(), words.next()) {
        if k == key {
            return Some(v);
        }
    }
    None
}

fn symbol(name: &str) -> Option<(&'static str, Class)> {
    SYMBOLS
        .iter()
        .find_map(|&(class, table)| Some((lookup(table, name)?, class)))
}

/// `c` in a `\mathbb`-style alphabet, from the Mathematical Alphanumeric
/// Symbols block and the letters that predate it in Letterlike Symbols.
fn alphabet(font: &str, c: char) -> char {
    let (upper, lower, digit, older) = match font {
        "mathbb" => (
            0x1D538,
            0x1D552,
            Some(0x1D7D8),
            "C ℂ H ℍ N ℕ P ℙ Q ℚ R ℝ Z ℤ",
        ),
        "mathcal" | "mathscr" => (
            0x1D49C,
            0x1D4B6,
            None,
            "B ℬ E ℰ F ℱ H ℋ I ℐ L ℒ M ℳ R ℛ e ℯ g ℊ o ℴ",
        ),
        "mathfrak" => (0x1D504, 0x1D51E, None, "C ℭ H ℌ I ℑ R ℜ Z ℨ"),
        _ => (0x1D400, 0x1D41A, Some(0x1D7CE), ""),
    };
    if let Some(old) = lookup(older, &c.to_string()).and_then(|s| s.chars().next()) {
        return old;
    }
    let code = match c {
        'A'..='Z' => Some(upper + (c as u32 - 'A' as u32)),
        'a'..='z' => Some(lower + (c as u32 - 'a' as u32)),
        '0'..='9' => digit.map(|d| d + (c as u32 - '0' as u32)),
        _ => None,
    };
    code.and_then(char::from_u32).unwrap_or(c)
}

/// Characters and their superscripts, as pairs.
const SUPERSCRIPTS: &str =
    "0 ⁰ 1 ¹ 2 ² 3 ³ 4 ⁴ 5 ⁵ 6 ⁶ 7 ⁷ 8 ⁸ 9 ⁹ + ⁺ − ⁻ = ⁼ ( ⁽ ) ⁾ a ᵃ b ᵇ c ᶜ \
    d ᵈ e ᵉ f ᶠ g ᵍ h ʰ i ⁱ j ʲ k ᵏ l ˡ m ᵐ n ⁿ o ᵒ p ᵖ r ʳ s ˢ t ᵗ u ᵘ v ᵛ w ʷ x ˣ y ʸ z ᶻ \
    A ᴬ B ᴮ D ᴰ E ᴱ G ᴳ H ᴴ I ᴵ J ᴶ K ᴷ L ᴸ M ᴹ N ᴺ O ᴼ P ᴾ R ᴿ T ᵀ U ᵁ V ⱽ W ᵂ α ᵅ β ᵝ \
    γ ᵞ δ ᵟ ε ᵋ θ ᶿ ι ᶥ φ ᵠ ϕ ᵠ χ ᵡ ∘ ° ′ ′ ″ ″ † † ‡ ‡ ∗ ∗ * *";

/// Characters and their subscripts, as pairs.
const SUBSCRIPTS: &str = "0 ₀ 1 ₁ 2 ₂ 3 ₃ 4 ₄ 5 ₅ 6 ₆ 7 ₇ 8 ₈ 9 ₉ + ₊ − ₋ = ₌ ( ₍ ) ₎ a ₐ e ₑ h ₕ \
    i ᵢ j ⱼ k ₖ l ₗ m ₘ n ₙ o ₒ p ₚ r ᵣ s ₛ t ₜ u ᵤ v ᵥ x ₓ β ᵦ γ ᵧ ρ ᵨ φ ᵩ ϕ ᵩ χ ᵪ , ,";

/// `c` as a superscript or subscript; one that already is (the `²` of
/// `e^{-x^2}`) stays as it is.
fn small(c: char, sup: bool) -> Option<String> {
    let table = if sup { SUPERSCRIPTS } else { SUBSCRIPTS };
    let c = c.to_string();
    let already = table.split_whitespace().skip(1).step_by(2).any(|v| v == c);
    if already {
        return Some(c);
    }
    lookup(table, &c).map(str::to_string)
}

/// A script in Unicode super- or subscript letters, or `^x` / `^(…)` when
/// some character has none.
fn script(text: &str, sup: bool) -> String {
    if let Some(small) = text.chars().map(|c| small(c, sup)).collect() {
        return small;
    }
    let mark = if sup { '^' } else { '_' };
    if text.chars().count() == 1 {
        format!("{mark}{text}")
    } else {
        format!("{mark}({text})")
    }
}

fn scripts_fit(text: &str, sup: bool) -> bool {
    text.chars().all(|c| small(c, sup).is_some())
}

/// The class a node spaces with on its left and on its right.
fn classes(node: &Node) -> (Class, Class) {
    match node {
        Node::Sym(_, c) => (*c, *c),
        Node::Scripts { base, .. } => classes(base),
        Node::Fenced(..) => (Class::Open, Class::Close),
        _ => (Class::Ord, Class::Ord),
    }
}

enum Piece<'a> {
    Node(&'a Node),
    Gap,
}

/// A row's items with the spaces TeX puts around operators and relations;
/// `compact` (scripts) leaves them out.
fn spaced(items: &[Node], compact: bool) -> Vec<Piece<'_>> {
    use Class::*;
    let mut out = Vec::new();
    let mut prev: Option<Class> = None;
    for node in items {
        if matches!(node, Node::Space(_)) {
            out.push(Piece::Node(node));
            prev = None;
            continue;
        }
        let (mut left, mut right) = classes(node);
        if left == Bin && matches!(prev, None | Some(Bin | Rel | Open | Punct | Op)) {
            // A sign, not an operation: `-x`, `(+1)`.
            left = Ord;
            if right == Bin {
                right = Ord;
            }
        }
        if let (Some(p), false) = (prev, compact) {
            let gap = (matches!(left, Bin | Rel)
                || matches!(p, Bin | Rel | Punct)
                || (p == Op && !matches!(left, Open | Close | Punct)))
                && !(p == Rel && left == Rel);
            if gap {
                out.push(Piece::Gap);
            }
        }
        out.push(Piece::Node(node));
        prev = Some(right);
    }
    out
}

fn linear(node: &Node, compact: bool) -> String {
    match node {
        Node::Sym(s, _) => s.clone(),
        Node::Space(n) => " ".repeat(*n),
        Node::Row(items) => spaced(items, compact)
            .iter()
            .map(|p| match p {
                Piece::Gap => " ".to_string(),
                Piece::Node(n) => linear(n, compact),
            })
            .collect(),
        Node::Frac(a, b, true) => {
            format!(
                "{}/{}",
                operand(a, compact, false),
                operand(b, compact, false)
            )
        }
        Node::Frac(a, b, false) => format!("C({}, {})", linear(a, true), linear(b, true)),
        Node::Sqrt(index, x) => {
            let index = index.as_ref().map(|i| linear(i, true));
            let root = match index.as_deref() {
                None => "√".to_string(),
                Some("3") => "∛".to_string(),
                Some("4") => "∜".to_string(),
                Some(i) => format!("{}√", script(i, true)),
            };
            root + &operand(x, compact, true)
        }
        Node::Scripts { base, sub, sup } => {
            let mut s = linear(base, compact);
            if let Some(sub) = sub {
                s += &script(&linear(sub, true), false);
            }
            if let Some(sup) = sup {
                s += &script(&linear(sup, true), true);
            }
            s
        }
        Node::Fenced(open, x, close) => format!("{open}{}{close}", linear(x, compact)),
        Node::Grid {
            rows,
            align,
            open,
            close,
        } => {
            let sep = if matches!(align, Align::RightLeft) {
                " "
            } else {
                ", "
            };
            let rows: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cells: Vec<String> = r.iter().map(|c| linear(c, compact)).collect();
                    cells.join(sep)
                })
                .collect();
            format!("{open}{}{close}", rows.join("; "))
        }
    }
}

/// `x` as a fraction part, or a root when `strict`: parenthesized when it
/// has operators, or for a root anything longer than one symbol.
fn operand(x: &Node, compact: bool, strict: bool) -> String {
    let s = linear(x, compact);
    if needs_parens(x, strict) {
        format!("({s})")
    } else {
        s
    }
}

fn needs_parens(x: &Node, strict: bool) -> bool {
    match x {
        Node::Row(items) if items.len() == 1 => needs_parens(&items[0], strict),
        Node::Row(items) => {
            strict
                || items.iter().enumerate().any(|(i, n)| match n {
                    Node::Space(_) | Node::Frac(..) | Node::Grid { .. } => true,
                    // A leading sign is part of the operand: `−b/2a`.
                    _ => {
                        let (left, _) = classes(n);
                        matches!(left, Class::Rel | Class::Punct | Class::Op)
                            || (left == Class::Bin && i > 0)
                    }
                })
        }
        Node::Frac(..) | Node::Grid { .. } => true,
        Node::Sym(s, _) => s.contains(' '),
        _ => false,
    }
}

/// Rows of equal display width; `base` is the row that lines up with the
/// text around it.
#[derive(Clone)]
struct Pic {
    rows: Vec<String>,
    base: usize,
    width: usize,
}

impl Pic {
    fn text(s: &str) -> Pic {
        Pic {
            rows: vec![s.to_string()],
            base: 0,
            width: s.width(),
        }
    }

    fn height(&self) -> usize {
        self.rows.len()
    }

    fn blank_row(&self) -> String {
        " ".repeat(self.width)
    }
}

/// `s` padded to `width`: `at` 0 is left, 1 center, 2 right.
fn pad(s: &str, width: usize, at: usize) -> String {
    let room = width.saturating_sub(s.width());
    let left = room * at / 2;
    format!("{}{s}{}", " ".repeat(left), " ".repeat(room - left))
}

/// Side by side, lined up on their base rows.
fn hcat(pics: &[Pic]) -> Pic {
    let above = pics.iter().map(|p| p.base).max().unwrap_or(0);
    let below = pics
        .iter()
        .map(|p| p.height() - p.base - 1)
        .max()
        .unwrap_or(0);
    let rows = (0..=above + below)
        .map(|r| {
            pics.iter()
                .map(|p| {
                    (r + p.base)
                        .checked_sub(above)
                        .and_then(|i| p.rows.get(i).cloned())
                        .unwrap_or_else(|| p.blank_row())
                })
                .collect()
        })
        .collect();
    Pic {
        rows,
        base: above,
        width: pics.iter().map(|p| p.width).sum(),
    }
}

/// One above another, centered; `base` counts from the top.
fn vstack(pics: &[Pic], base: usize) -> Pic {
    let width = pics.iter().map(|p| p.width).max().unwrap_or(0);
    Pic {
        rows: pics
            .iter()
            .flat_map(|p| p.rows.iter().map(|r| pad(r, width, 1)))
            .collect(),
        base,
        width,
    }
}

/// A bracket `height` rows tall.
fn delimiter(d: &str, height: usize, base: usize) -> Pic {
    let (top, mid, bottom) = match d {
        "(" => ('⎛', '⎜', '⎝'),
        ")" => ('⎞', '⎟', '⎠'),
        "[" => ('⎡', '⎢', '⎣'),
        "]" => ('⎤', '⎥', '⎦'),
        "{" => ('⎧', '⎪', '⎩'),
        "}" => ('⎫', '⎪', '⎭'),
        "⌊" => ('⎢', '⎢', '⎣'),
        "⌋" => ('⎥', '⎥', '⎦'),
        "⌈" => ('⎡', '⎢', '⎢'),
        "⌉" => ('⎤', '⎥', '⎥'),
        "|" => ('│', '│', '│'),
        "‖" => ('‖', '‖', '‖'),
        "" => {
            return Pic {
                rows: vec![String::new(); height],
                base,
                width: 0,
            }
        }
        other => {
            let blank = " ".repeat(other.width());
            let rows = (0..height)
                .map(|r| {
                    if r == base {
                        other.to_string()
                    } else {
                        blank.clone()
                    }
                })
                .collect();
            return Pic {
                rows,
                base,
                width: other.width(),
            };
        }
    };
    let rows = (0..height)
        .map(|r| {
            let c = match r {
                0 => top,
                r if r + 1 == height => bottom,
                r if height > 2 && r == (height - 1) / 2 && matches!(d, "{" | "}") => {
                    if d == "{" {
                        '⎨'
                    } else {
                        '⎬'
                    }
                }
                _ => mid,
            };
            c.to_string()
        })
        .collect();
    Pic {
        rows,
        base,
        width: 1,
    }
}

fn fenced(open: &str, inner: Pic, close: &str) -> Pic {
    if inner.height() == 1 {
        return hcat(&[Pic::text(open), inner, Pic::text(close)]);
    }
    let (h, b) = (inner.height(), inner.base);
    hcat(&[delimiter(open, h, b), inner, delimiter(close, h, b)])
}

fn layout(node: &Node) -> Pic {
    match node {
        Node::Sym(s, _) => Pic::text(s),
        Node::Space(n) => Pic::text(&" ".repeat(*n)),
        Node::Row(items) if items.is_empty() => Pic::text(""),
        Node::Row(items) => {
            let pics: Vec<Pic> = spaced(items, false)
                .iter()
                .map(|p| match p {
                    Piece::Gap => Pic::text(" "),
                    Piece::Node(n) => layout(n),
                })
                .collect();
            hcat(&pics)
        }
        Node::Frac(a, b, bar) => {
            let (a, b) = (layout(a), layout(b));
            if !bar {
                let base = a.height().saturating_sub(1);
                return fenced("(", vstack(&[a, b], base), ")");
            }
            let rule = Pic::text(&"─".repeat(a.width.max(b.width) + 2));
            let base = a.height();
            vstack(&[a, rule, b], base)
        }
        Node::Sqrt(index, x) => {
            let body = layout(x);
            let index = index.as_ref().map(|i| linear(i, true));
            if body.height() == 1 && !needs_parens(x, true) {
                let root = match index.as_deref() {
                    None => "√".to_string(),
                    Some("3") => "∛".to_string(),
                    Some("4") => "∜".to_string(),
                    Some(i) => format!("{}√", script(i, true)),
                };
                return Pic::text(&(root + &body.rows[0]));
            }
            // A radical sign as tall as the body, with a bar over it.
            let h = body.height();
            let mut rows = vec![format!(
                "{}{}",
                " ".repeat(h + 1),
                "_".repeat(body.width + 1)
            )];
            for (i, row) in body.rows.iter().enumerate() {
                // The rising stroke is one column further left on each row down.
                let at = h - i;
                let lead = if at == 1 {
                    "╲".to_string()
                } else {
                    " ".repeat(at)
                };
                rows.push(format!("{lead}╱{}{row}", " ".repeat(h + 1 - at)));
            }
            let mut pic = Pic {
                rows,
                base: body.base + 1,
                width: h + 2 + body.width,
            };
            if let Some(i) = index {
                let i = script(&i, true);
                let blank = " ".repeat(i.width());
                for (r, row) in pic.rows.iter_mut().enumerate() {
                    let lead = if r + 2 == h + 1 { &i } else { &blank };
                    *row = format!("{lead}{row}");
                }
                pic.width += i.width();
            }
            pic
        }
        Node::Scripts { base, sub, sup } => {
            let b = layout(base);
            let sub_text = sub.as_ref().map(|s| linear(s, true));
            let sup_text = sup.as_ref().map(|s| linear(s, true));
            if takes_limits(base) {
                let mut parts = Vec::new();
                parts.extend(sup_text.as_deref().map(Pic::text));
                let row = parts.len() + b.base;
                parts.push(b);
                parts.extend(sub_text.as_deref().map(Pic::text));
                return vstack(&parts, row);
            }
            let fits = sub_text.as_deref().is_none_or(|s| scripts_fit(s, false))
                && sup_text.as_deref().is_none_or(|s| scripts_fit(s, true));
            if b.height() == 1 && fits {
                return Pic::text(&linear(node, false));
            }
            // Above and below the base, on its right.
            let above = sup_text.as_deref().map(Pic::text);
            let below = sub_text.as_deref().map(Pic::text);
            let w = above
                .iter()
                .chain(&below)
                .map(|p| p.width)
                .max()
                .unwrap_or(0);
            let lift = above.as_ref().map_or(0, Pic::height);
            let mut rows: Vec<String> = Vec::new();
            for p in above.iter() {
                rows.extend(p.rows.iter().map(|r| b.blank_row() + &pad(r, w, 0)));
            }
            rows.extend(b.rows.iter().map(|r| format!("{r}{}", " ".repeat(w))));
            for p in below.iter() {
                rows.extend(p.rows.iter().map(|r| b.blank_row() + &pad(r, w, 0)));
            }
            Pic {
                rows,
                base: lift + b.base,
                width: b.width + w,
            }
        }
        Node::Fenced(open, x, close) => fenced(open, layout(x), close),
        Node::Grid {
            rows,
            align,
            open,
            close,
        } => {
            let cells: Vec<Vec<Pic>> = rows
                .iter()
                .map(|r| r.iter().map(layout).collect())
                .collect();
            let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
            let widths: Vec<usize> = (0..columns)
                .map(|c| {
                    cells
                        .iter()
                        .filter_map(|r| r.get(c))
                        .map(|p| p.width)
                        .max()
                        .unwrap_or(0)
                })
                .collect();
            let gap = match align {
                Align::RightLeft => " ",
                _ => "  ",
            };
            let mut lines = Vec::new();
            for row in &cells {
                let placed: Vec<Pic> = (0..columns)
                    .map(|c| {
                        let at = match align {
                            Align::Center => 1,
                            Align::Left => 0,
                            Align::RightLeft => 2 * ((c + 1) % 2),
                        };
                        let cell = row.get(c).cloned().unwrap_or_else(|| Pic::text(""));
                        let mut cell = Pic {
                            rows: cell.rows.iter().map(|r| pad(r, widths[c], at)).collect(),
                            width: widths[c],
                            ..cell
                        };
                        if c + 1 < columns {
                            cell.rows.iter_mut().for_each(|r| r.push_str(gap));
                            cell.width += gap.width();
                        }
                        cell
                    })
                    .collect();
                lines.extend(hcat(&placed).rows);
            }
            let width = lines.first().map_or(0, |l| l.width());
            let height = lines.len();
            let grid = Pic {
                rows: lines,
                base: height.saturating_sub(1) / 2,
                width,
            };
            if open.is_empty() && close.is_empty() {
                grid
            } else {
                fenced(open, grid, close)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_math_reads_as_unicode() {
        let cases = [
            (r"e^{i\pi} + 1 = 0", "e^(iπ) + 1 = 0"),
            (r"x_i^2 - y_{n+1} + e^{-x^2}", "xᵢ² − yₙ₊₁ + e⁻ˣ²"),
            (r"-b \pm \sqrt{b^2-4ac}", "−b ± √(b² − 4ac)"),
            (r"\frac{a+b}{2} \leq \frac{-1}{n}", "(a + b)/2 ≤ −1/n"),
            (r"f(x,y) = \sin x \cdot \cos(y)", "f(x, y) = sin x ⋅ cos(y)"),
            (r"\forall \epsilon > 0, \exists \delta", "∀ϵ > 0, ∃δ"),
            (r"x \in \mathbb{R}^n", "x ∈ ℝⁿ"),
            (
                r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
                "∑ᵢ₌₁ⁿ i = n(n + 1)/2",
            ),
            (r"a^{q} \text{ if } a \neq 0", "a^q if a ≠ 0"),
            (r"\hat{x} \to \infty", "x̂ → ∞"),
            (r"\binom{n}{k} \not= 0", "C(n, k) ≠ 0"),
            (r"\unknown x", r"\unknown x"),
        ];
        for (tex, expected) in cases {
            assert_eq!(inline(tex), expected, "{tex}");
        }
    }

    #[test]
    fn display_math_stacks_fractions_roots_limits_and_matrices() {
        let cases: &[(&str, &[&str])] = &[
            (r"x = \frac{-b}{2a}", &["     −b", "x = ────", "     2a"]),
            (r"\sum_{i=1}^{n} i^2", &[" n", " ∑  i²", "i=1"]),
            (
                r"\sqrt{\frac{a}{b}}",
                &["    ____", "   ╱  a", "  ╱  ───", "╲╱    b"],
            ),
            (
                r"A = \begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}",
                &["A = ⎛1  0⎞", "    ⎝0  1⎠"],
            ),
            (
                r"|x| = \begin{cases} x & x \geq 0 \\ -x & x < 0 \end{cases}",
                &["|x| = ⎧x   x ≥ 0", "      ⎩−x  x < 0"],
            ),
            (r"E = mc^2", &["E = mc²"]),
        ];
        for (tex, expected) in cases {
            assert_eq!(display(tex), *expected, "{tex}");
        }
    }
}
