//! The formula AST and its textual form.
//!
//! Everything on the board is one `F`. Rendering is fully deterministic and
//! returns *tokens* annotated with the AST path they came from, so the UI can
//! highlight a redex or every occurrence of an atom without re-parsing.

/// Atom identifier; index into the fixed name table `pqrstuvw`.
pub type Atom = u8;

pub const ATOM_NAMES: &[char] = &['p', 'q', 'r', 's', 't', 'u', 'v', 'w'];

pub fn atom_name(a: Atom) -> char {
    ATOM_NAMES[a as usize]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    And,
    Or,
    Imp,
    Eq,
}

impl Op {
    pub fn glyph(self) -> char {
        match self {
            Op::And => '∧',
            Op::Or => '∨',
            Op::Imp => '⇒',
            Op::Eq => '=',
        }
    }

    /// Chains of the same operator render without parentheses.
    pub fn chains(self) -> bool {
        matches!(self, Op::And | Op::Or)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum F {
    Const(bool),
    Atom(Atom),
    Not(Box<F>),
    Bin(Op, Box<F>, Box<F>),
}

/// A path from the root: 0 = left (or the child of ¬), 1 = right.
pub type Path = Vec<u8>;

impl F {
    #[allow(clippy::should_implement_trait)]
    pub fn not(f: F) -> F {
        F::Not(Box::new(f))
    }
    pub fn bin(op: Op, l: F, r: F) -> F {
        F::Bin(op, Box::new(l), Box::new(r))
    }
    pub fn and(l: F, r: F) -> F {
        F::bin(Op::And, l, r)
    }
    pub fn or(l: F, r: F) -> F {
        F::bin(Op::Or, l, r)
    }
    pub fn imp(l: F, r: F) -> F {
        F::bin(Op::Imp, l, r)
    }
    pub fn eq(l: F, r: F) -> F {
        F::bin(Op::Eq, l, r)
    }

    pub fn as_const(&self) -> Option<bool> {
        match self {
            F::Const(b) => Some(*b),
            _ => None,
        }
    }

    /// Distinct atoms present, in ascending order.
    pub fn atoms(&self) -> Vec<Atom> {
        let mut seen = [false; 8];
        self.visit(&mut |f| {
            if let F::Atom(a) = f {
                seen[*a as usize] = true;
            }
        });
        (0..8).filter(|&i| seen[i as usize]).collect()
    }

    pub fn count_atom(&self, a: Atom) -> usize {
        let mut n = 0;
        self.visit(&mut |f| {
            if let F::Atom(x) = f {
                if *x == a {
                    n += 1;
                }
            }
        });
        n
    }

    pub fn total_occurrences(&self) -> usize {
        let mut n = 0;
        self.visit(&mut |f| {
            if matches!(f, F::Atom(_)) {
                n += 1;
            }
        });
        n
    }

    pub fn size(&self) -> usize {
        let mut n = 0;
        self.visit(&mut |_| n += 1);
        n
    }

    fn visit(&self, f: &mut impl FnMut(&F)) {
        f(self);
        match self {
            F::Not(x) => x.visit(f),
            F::Bin(_, l, r) => {
                l.visit(f);
                r.visit(f);
            }
            _ => {}
        }
    }

    /// Replace every occurrence of `atom` with the constant `value`.
    pub fn substitute(&self, atom: Atom, value: bool) -> F {
        match self {
            F::Const(b) => F::Const(*b),
            F::Atom(a) => {
                if *a == atom {
                    F::Const(value)
                } else {
                    F::Atom(*a)
                }
            }
            F::Not(x) => F::not(x.substitute(atom, value)),
            F::Bin(op, l, r) => F::bin(*op, l.substitute(atom, value), r.substitute(atom, value)),
        }
    }

    /// Subtree at `path`, if it exists.
    pub fn at(&self, path: &[u8]) -> Option<&F> {
        let mut cur = self;
        for &step in path {
            cur = match cur {
                F::Not(x) => {
                    if step == 0 {
                        x
                    } else {
                        return None;
                    }
                }
                F::Bin(_, l, r) => {
                    if step == 0 {
                        l
                    } else {
                        r
                    }
                }
                _ => return None,
            };
        }
        Some(cur)
    }

    /// Truth-table evaluation under a total assignment (bit i of `env` = atom i).
    pub fn eval(&self, env: u8) -> bool {
        match self {
            F::Const(b) => *b,
            F::Atom(a) => env & (1 << a) != 0,
            F::Not(x) => !x.eval(env),
            F::Bin(op, l, r) => {
                let (a, b) = (l.eval(env), r.eval(env));
                match op {
                    Op::And => a && b,
                    Op::Or => a || b,
                    Op::Imp => !a || b,
                    Op::Eq => a == b,
                }
            }
        }
    }

    /// Compact canonical string, used as a solver memo key and in tests.
    pub fn key(&self) -> String {
        let mut s = String::new();
        self.key_into(&mut s);
        s
    }

    fn key_into(&self, s: &mut String) {
        match self {
            F::Const(true) => s.push('T'),
            F::Const(false) => s.push('F'),
            F::Atom(a) => s.push(atom_name(*a)),
            F::Not(x) => {
                s.push('!');
                x.key_into(s);
            }
            F::Bin(op, l, r) => {
                s.push('(');
                l.key_into(s);
                s.push(match op {
                    Op::And => '&',
                    Op::Or => '|',
                    Op::Imp => '>',
                    Op::Eq => '=',
                });
                r.key_into(s);
                s.push(')');
            }
        }
    }
}

/// One display glyph with provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub ch: char,
    /// Path of the AST node this glyph belongs to.
    pub path: Path,
    /// Set when this glyph is an atom occurrence.
    pub atom: Option<Atom>,
    /// True for spacing glyphs (excluded from the glyph budget).
    pub space: bool,
}

/// Render to tokens. Fully parenthesized except: outermost, atoms/constants,
/// ¬-groups, and same-op chains of ∧/∨ (which read flat, as on the rules card).
pub fn tokens(f: &F) -> Vec<Token> {
    let mut out = Vec::new();
    emit(f, &mut Vec::new(), &mut out, None);
    out
}

fn emit(f: &F, path: &mut Path, out: &mut Vec<Token>, parent: Option<Op>) {
    let tok = |ch: char, path: &Path, atom: Option<Atom>, space: bool| Token {
        ch,
        path: path.clone(),
        atom,
        space,
    };
    match f {
        F::Const(b) => out.push(tok(if *b { '⊤' } else { '⊥' }, path, None, false)),
        F::Atom(a) => out.push(tok(atom_name(*a), path, Some(*a), false)),
        F::Not(x) => {
            out.push(tok('¬', path, None, false));
            let wrap = matches!(**x, F::Bin(..));
            if wrap {
                out.push(tok('(', path, None, false));
            }
            path.push(0);
            emit(x, path, out, None);
            path.pop();
            if wrap {
                out.push(tok(')', path, None, false));
            }
        }
        F::Bin(op, l, r) => {
            let needs_paren = |child: &F| -> bool {
                match child {
                    F::Bin(cop, ..) => !(op.chains() && *cop == *op),
                    _ => false,
                }
            };
            let wrap_self = parent.is_some();
            if wrap_self {
                out.push(tok('(', path, None, false));
            }
            // Children of a chained op keep the chain flat; anything else is
            // rendered inside its own parens via the recursive call.
            let lp = needs_paren(l);
            path.push(0);
            emit(l, path, out, if lp { Some(*op) } else { None });
            path.pop();
            out.push(tok(' ', path, None, true));
            out.push(tok(op.glyph(), path, None, false));
            out.push(tok(' ', path, None, true));
            let rp = needs_paren(r);
            path.push(1);
            emit(r, path, out, if rp { Some(*op) } else { None });
            path.pop();
            if wrap_self {
                out.push(tok(')', path, None, false));
            }
        }
    }
}

/// Plain string rendering (what the player reads).
pub fn pretty(f: &F) -> String {
    tokens(f).iter().map(|t| t.ch).collect()
}

/// Number of non-space glyphs (the legibility budget from the design doc).
pub fn glyph_count(f: &F) -> usize {
    tokens(f).iter().filter(|t| !t.space).count()
}

/// Token index span (start..end, exclusive) of the subtree at `path`.
/// Includes any parens/¬ that belong to that node.
pub fn span_of(toks: &[Token], path: &[u8]) -> Option<(usize, usize)> {
    let mut start = None;
    let mut end = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.path.len() >= path.len() && t.path[..path.len()] == *path {
            if start.is_none() {
                start = Some(i);
            }
            end = i + 1;
        }
    }
    start.map(|s| (s, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> F {
        F::Atom(0)
    }
    fn q() -> F {
        F::Atom(1)
    }
    fn r() -> F {
        F::Atom(2)
    }

    #[test]
    fn pretty_worked_example() {
        // (p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q), left-associated chain.
        let f = F::and(F::and(F::imp(p(), q()), F::or(p(), r())), F::imp(r(), q()));
        assert_eq!(pretty(&f), "(p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q)");
    }

    #[test]
    fn pretty_not_and_nesting() {
        let f = F::or(F::not(F::and(p(), q())), F::not(r()));
        assert_eq!(pretty(&f), "¬(p ∧ q) ∨ ¬r");
        let g = F::imp(F::imp(p(), q()), r());
        assert_eq!(pretty(&g), "(p ⇒ q) ⇒ r");
        let h = F::and(p(), F::or(q(), r()));
        assert_eq!(pretty(&h), "p ∧ (q ∨ r)");
    }

    #[test]
    fn chains_stay_flat_both_sides() {
        let left = F::and(F::and(p(), q()), r());
        let right = F::and(p(), F::and(q(), r()));
        assert_eq!(pretty(&left), "p ∧ q ∧ r");
        assert_eq!(pretty(&right), "p ∧ q ∧ r");
    }

    #[test]
    fn substitution_hits_every_occurrence() {
        let f = F::and(F::imp(p(), q()), F::or(p(), r()));
        let g = f.substitute(0, true);
        assert_eq!(g.count_atom(0), 0);
        assert_eq!(pretty(&g), "(⊤ ⇒ q) ∧ (⊤ ∨ r)");
    }

    #[test]
    fn spans_are_contiguous_and_correct() {
        let f = F::and(F::imp(p(), q()), F::or(p(), r()));
        let toks = tokens(&f);
        let (s, e) = span_of(&toks, &[1]).unwrap();
        let text: String = toks[s..e].iter().map(|t| t.ch).collect();
        assert_eq!(text, "(p ∨ r)");
        let (s, e) = span_of(&toks, &[0, 0]).unwrap();
        let text: String = toks[s..e].iter().map(|t| t.ch).collect();
        assert_eq!(text, "p");
    }

    #[test]
    fn eval_matches_semantics() {
        let f = F::imp(p(), q());
        assert!(f.eval(0b00)); // F ⇒ F = T
        assert!(f.eval(0b10)); // F ⇒ T = T
        assert!(!f.eval(0b01)); // T ⇒ F = F
        assert!(f.eval(0b11));
        let g = F::eq(p(), q());
        assert!(g.eval(0b00));
        assert!(!g.eval(0b01));
    }
}
