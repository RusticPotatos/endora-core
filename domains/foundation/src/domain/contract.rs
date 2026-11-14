//! The shape of a call: the fields it takes, the facts it returns, and whether its executor
//! enforces the budget Core grants. Pure rules; a capability declares them, Core checks them.
//!
//! An input is the call's text. With `n` declared input fields it is `n` values separated by
//! a newline, in declaration order; the last may span lines. With none declared it is empty.

use super::decide::CallSpec;
use super::hold::{Fact, FactValue};

/// Longest word a `Word` type may allow.
pub const MOST_WORD: usize = 32;
/// Longest value a `Text` input field may carry, in bytes.
pub const MOST_TEXT: usize = 16 * 1024;
/// Seconds a call may take when its spec does not say (the budget's `max_seconds`).
pub const DEFAULT_MAX_SECONDS: u64 = 600;

/// What a field's value may be. `Text` is for inputs only: a fact is never free text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    /// A whole number.
    Int,
    /// `true` or `false`.
    Bool,
    /// One of a closed list of words, each at most 32 characters of `[a-z0-9_-]`.
    Word(Vec<String>),
    /// Free text (input fields only).
    Text,
}

/// One named, typed field of a call's input or of its result's facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The field's name.
    pub name: String,
    /// What it may hold.
    pub ty: ValueType,
}

impl Field {
    /// A number field.
    #[must_use]
    pub fn int(name: &str) -> Self {
        Self::of(name, ValueType::Int)
    }
    /// A yes/no field.
    #[must_use]
    pub fn bool(name: &str) -> Self {
        Self::of(name, ValueType::Bool)
    }
    /// A field holding one of `words`.
    #[must_use]
    pub fn word(name: &str, words: &[&str]) -> Self {
        Self::of(
            name,
            ValueType::Word(words.iter().map(|w| (*w).to_owned()).collect()),
        )
    }
    /// A free-text input field.
    #[must_use]
    pub fn text(name: &str) -> Self {
        Self::of(name, ValueType::Text)
    }
    fn of(name: &str, ty: ValueType) -> Self {
        Self {
            name: name.to_owned(),
            ty,
        }
    }

    /// Whether `value` is a legal value of this field's type.
    fn accepts(&self, value: &str) -> Result<(), String> {
        let ok = match &self.ty {
            ValueType::Int => value.trim().parse::<i64>().is_ok(),
            ValueType::Bool => matches!(value.trim(), "true" | "false"),
            ValueType::Word(list) => list.iter().any(|w| w == value.trim()),
            ValueType::Text => !value.trim().is_empty() && value.len() <= MOST_TEXT,
        };
        if ok {
            Ok(())
        } else {
            let shown: String = value.chars().take(40).collect();
            Err(format!(
                "field {} is not a valid {}: {shown:?}",
                self.name,
                match &self.ty {
                    ValueType::Int => "number".to_owned(),
                    ValueType::Bool => "true/false".to_owned(),
                    ValueType::Word(l) => format!("word of [{}]", l.join(", ")),
                    ValueType::Text => "non-empty text".to_owned(),
                }
            ))
        }
    }
}

/// Whether the call's executor stops itself at the budget Core grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetMode {
    /// It cannot: such a call may only declare a maximum cost of zero.
    #[default]
    Unenforced,
    /// It does, up to `units` micro-USD per call.
    Enforced {
        /// The most the executor can enforce for one call, in micro-USD (1 USD = 1,000,000).
        units: u64,
    },
}

/// The budget Core grants one call, passed to the capability as a typed input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// Most it may cost, in micro-USD (1 USD = 1,000,000) (the call's declared maximum).
    pub max_cost_micros: u64,
    /// Most seconds it may take.
    pub max_seconds: u64,
    /// Most model turns, when the call has turns.
    pub max_turns: Option<u32>,
}

impl CallSpec {
    /// This declaration with these input fields.
    #[must_use]
    pub fn with_inputs(mut self, inputs: Vec<Field>) -> Self {
        self.inputs = inputs;
        self
    }

    /// This declaration with these output facts.
    #[must_use]
    pub fn with_facts(mut self, facts: Vec<Field>) -> Self {
        self.facts = facts;
        self
    }

    /// Whether a non-zero maximum cost is allowed: only when the executor enforces a budget at
    /// least that large. A zero maximum is always allowed.
    #[must_use]
    pub const fn budget_is_enforceable(&self) -> bool {
        if self.max_cost_micros == 0 {
            return true;
        }
        match self.budget {
            BudgetMode::Enforced { units } => self.max_cost_micros <= units,
            BudgetMode::Unenforced => false,
        }
    }

    /// The declaration itself must be well formed.
    ///
    /// # Errors
    /// A sentence naming the offending field.
    pub fn validate_shape(&self) -> Result<(), String> {
        for (what, fields) in [("input", &self.inputs), ("fact", &self.facts)] {
            let mut seen: Vec<&str> = Vec::new();
            for f in fields {
                if f.name.trim().is_empty() || seen.contains(&f.name.as_str()) {
                    return Err(format!(
                        "call {} declares a blank or repeated {what} name {:?}",
                        self.name, f.name
                    ));
                }
                seen.push(&f.name);
                if what == "fact" && f.ty == ValueType::Text {
                    return Err(format!(
                        "call {}: fact {} is free text, and a fact never is",
                        self.name, f.name
                    ));
                }
                if let ValueType::Word(list) = &f.ty {
                    let bad = list.is_empty()
                        || list.iter().any(|w| {
                            w.is_empty()
                                || w.len() > MOST_WORD
                                || !w.chars().all(|c| {
                                    c.is_ascii_lowercase()
                                        || c.is_ascii_digit()
                                        || c == '_'
                                        || c == '-'
                                })
                        });
                    if bad {
                        return Err(format!(
                            "call {}: {what} {} needs a closed list of words, each at most {MOST_WORD} characters of a-z 0-9 _ -",
                            self.name, f.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether `input` matches the declared input fields.
    ///
    /// # Errors
    /// A sentence saying what does not match.
    pub fn check_input(&self, input: &str) -> Result<(), String> {
        let n = self.inputs.len();
        if n == 0 {
            return if input.trim().is_empty() {
                Ok(())
            } else {
                Err(format!("call {} takes no input", self.name))
            };
        }
        let parts: Vec<&str> = input.splitn(n, '\n').collect();
        if parts.len() != n {
            return Err(format!(
                "call {} takes {n} input fields ({}), one per line",
                self.name,
                self.inputs
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for (f, v) in self.inputs.iter().zip(parts) {
            f.accepts(v)?;
        }
        Ok(())
    }

    /// Whether every fact is declared, of its declared type and (for a word) on its list.
    ///
    /// # Errors
    /// A sentence naming the fact.
    pub fn check_facts(&self, facts: &[Fact]) -> Result<(), String> {
        for fact in facts {
            let Some(f) = self.facts.iter().find(|f| f.name == fact.name) else {
                return Err(format!(
                    "fact {} is not declared by {}",
                    fact.name, self.name
                ));
            };
            let typed = matches!(
                (&f.ty, &fact.value),
                (ValueType::Int, FactValue::Int(_))
                    | (ValueType::Bool, FactValue::Bool(_))
                    | (ValueType::Word(_), FactValue::Word(_))
            );
            if !typed {
                return Err(format!("fact {} is not of its declared type", fact.name));
            }
            f.accepts(&fact.rendered())
                .map_err(|e| format!("fact {e}"))?;
            if let (ValueType::Word(_), FactValue::Word(w)) = (&f.ty, &fact.value) {
                // `accepts` trims: a word with stray whitespace is still not on the list.
                if w.trim() != w {
                    return Err(format!("fact {} is not on its declared list", fact.name));
                }
            }
        }
        Ok(())
    }

    /// Whether a check on `field` expecting `expect` makes sense for this call's declared facts.
    ///
    /// # Errors
    /// A sentence naming the field.
    pub fn check_expectation(&self, field: &str, expect: &str) -> Result<(), String> {
        let Some(f) = self.facts.iter().find(|f| f.name == field) else {
            return Err(format!("fact {field} is not declared by {}", self.name));
        };
        if f.ty != ValueType::Text && expect.trim() != expect {
            return Err(format!("expected value of {field} has stray whitespace"));
        }
        f.accepts(expect)
    }
}

#[cfg(test)]
mod tests {
    use endora_kernel::Reversibility;

    use super::*;

    #[test]
    fn a_zero_maximum_needs_no_enforcement_and_a_non_zero_one_needs_enough_units() {
        let mut s = CallSpec::new("a.b", Reversibility::Observe);
        assert!(s.budget_is_enforceable());
        s.max_cost_micros = 3;
        assert!(!s.budget_is_enforceable());
        s.budget = BudgetMode::Enforced { units: 2 };
        assert!(!s.budget_is_enforceable());
        s.budget = BudgetMode::Enforced { units: 3 };
        assert!(s.budget_is_enforceable());
    }
}
