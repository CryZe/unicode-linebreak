use crate::{break_class, BreakClass};
use std::mem;
use unicode_categories::UnicodeCategories;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Operator {
    Unknown,
    Could,
    Never,
    Must,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BreakOpportunity {
    Could,
    Must,
}

pub trait Tailoring {
    fn resolve(value: char) -> BreakClass;
    fn breaks_between_inseperable() -> bool {
        false
    }
}

#[derive(Copy, Clone)]
pub struct Strict;

impl Tailoring for Strict {
    fn resolve(value: char) -> BreakClass {
        match break_class(value as _) {
            BreakClass::Ambiguous | BreakClass::Surrogate | BreakClass::Unknown => {
                BreakClass::Alphabetic
            }
            BreakClass::ComplexContext => {
                if value.is_mark_spacing_combining() || value.is_mark_nonspacing() {
                    BreakClass::CombiningMark
                } else {
                    BreakClass::Alphabetic
                }
            }
            BreakClass::ConditionalJapaneseStarter => BreakClass::NonStarter,
            class => class,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Normal;

impl Tailoring for Normal {
    fn resolve(value: char) -> BreakClass {
        match break_class(value as _) {
            BreakClass::Ambiguous | BreakClass::Surrogate | BreakClass::Unknown => {
                BreakClass::Alphabetic
            }
            BreakClass::ComplexContext => {
                if value.is_mark_spacing_combining() || value.is_mark_nonspacing() {
                    BreakClass::CombiningMark
                } else {
                    BreakClass::Alphabetic
                }
            }
            BreakClass::ConditionalJapaneseStarter => BreakClass::Ideographic,
            class => class,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Loose;

impl Tailoring for Loose {
    fn resolve(value: char) -> BreakClass {
        match value {
            '\u{3005}' | '\u{303B}' | '\u{309D}' | '\u{309E}' | '\u{30FD}' | '\u{30FE}' => {
                BreakClass::Unknown
            }
            value => Normal::resolve(value),
        }
    }

    fn breaks_between_inseperable() -> bool {
        true
    }
}

impl BreakOpportunity {
    pub fn must_break(self) -> bool {
        match self {
            BreakOpportunity::Must => true,
            _ => false,
        }
    }
    pub fn could_break(self) -> bool {
        match self {
            BreakOpportunity::Could => true,
            _ => false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AnnotatedVec {
    operators: Vec<Operator>,
    classes: Vec<BreakClass>,
}

impl AnnotatedVec {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn clear(&mut self) {
        self.classes.clear();
        self.operators.clear();
    }

    pub fn extend_str<T: Tailoring>(&mut self, text: &str, _tailoring: T) {
        // LB1
        self.classes.extend(text.chars().map(T::resolve));
        self.operators.resize(self.classes.len(), Operator::Unknown);

        // LB2 + LB3
        if let Some(o) = self.operators.last_mut() {
            o.transition(Operator::Must);
        }

        // LB4
        transition_after(self, |class| class == BreakClass::Mandatory, Operator::Must);

        // LB5
        transition_between(
            self,
            |a, b| (BreakClass::CarriageReturn, BreakClass::LineFeed) == (a, b),
            Operator::Never,
        );
        transition_after(
            self,
            |class| match class {
                BreakClass::CarriageReturn | BreakClass::LineFeed | BreakClass::NextLine => true,
                _ => false,
            },
            Operator::Must,
        );

        // LB6 + LB7
        transition_before(
            self,
            |class| match class {
                BreakClass::Mandatory
                | BreakClass::CarriageReturn
                | BreakClass::LineFeed
                | BreakClass::NextLine
                | BreakClass::Space
                | BreakClass::ZeroWidthSpace => true,
                _ => false,
            },
            Operator::Never,
        );

        // LB8
        transition_after_last_space(
            self,
            |class| class == BreakClass::ZeroWidthSpace,
            Operator::Could,
        );

        // LB8a
        transition_after(
            self,
            |class| class == BreakClass::ZeroWidthJoiner,
            Operator::Never,
        );

        // LB9
        self.iter_mut()
            .filter(|c| *c.class == BreakClass::ZeroWidthJoiner)
            .for_each(|c| *c.class = BreakClass::CombiningMark);

        // Do regional indicators early so we still know what a combining mark is.
        // LB30a
        {
            let mut found_first = false;
            for i in 0..self.len() {
                if !found_first {
                    found_first = self.classes[i] == BreakClass::RegionalIndicator;
                } else {
                    match self.classes[i] {
                        BreakClass::CombiningMark => continue,
                        BreakClass::RegionalIndicator => {
                            self.operators[i - 1].transition(Operator::Never);
                            found_first = false;
                        }
                        _ => found_first = false,
                    }
                }
            }
        }

        // Replace all the combining marks now (some here and the rest directly
        // after in LB10).
        self.pairs(|mut a, b| match a.class {
            BreakClass::Mandatory
            | BreakClass::CarriageReturn
            | BreakClass::LineFeed
            | BreakClass::NextLine
            | BreakClass::Space
            | BreakClass::ZeroWidthSpace => {}
            _ => {
                if BreakClass::CombiningMark == *b.class {
                    a.transition(Operator::Never);
                    *b.class = *a.class;
                }
            }
        });

        // LB10
        self.iter_mut()
            .filter(|c| *c.class == BreakClass::CombiningMark)
            .for_each(|c| *c.class = BreakClass::Alphabetic);

        // LB11
        transition_around(
            self,
            |class| class == BreakClass::WordJoiner,
            Operator::Never,
        );

        // LB12
        transition_after(
            self,
            |class| class == BreakClass::NonBreakingGlue,
            Operator::Never,
        );

        // LB12a
        transition_between(
            self,
            |a, b| match a {
                BreakClass::Space | BreakClass::After | BreakClass::Hyphen => false,
                _ => b == BreakClass::NonBreakingGlue,
            },
            Operator::Never,
        );

        // LB13
        transition_before(
            self,
            |class| class == BreakClass::Exclamation,
            Operator::Never,
        );
        transition_between(
            self,
            |a, b| match b {
                BreakClass::ClosePunctuation
                | BreakClass::CloseParenthesis
                | BreakClass::InfixSeparator
                | BreakClass::Symbol => a != BreakClass::Numeric,
                _ => false,
            },
            Operator::Never,
        );

        // LB14
        transition_after_last_space(
            self,
            |class| class == BreakClass::OpenPunctuation,
            Operator::Never,
        );

        // LB15
        transition_after_last_space_if(
            self,
            |class| class == BreakClass::Quotation,
            |class| class == BreakClass::OpenPunctuation,
            Operator::Never,
        );

        // LB16
        transition_after_last_space_if(
            self,
            |class| class == BreakClass::ClosePunctuation || class == BreakClass::CloseParenthesis,
            |class| class == BreakClass::NonStarter,
            Operator::Never,
        );

        // LB17
        transition_after_last_space_if(
            self,
            |class| class == BreakClass::BeforeAndAfter,
            |class| class == BreakClass::BeforeAndAfter,
            Operator::Never,
        );

        // LB18
        transition_after(self, |class| class == BreakClass::Space, Operator::Could);

        // LB19
        transition_around(
            self,
            |class| class == BreakClass::Quotation,
            Operator::Never,
        );

        // LB20
        transition_around(
            self,
            |class| class == BreakClass::Contingent,
            Operator::Could,
        );

        // LB21
        transition_before(
            self,
            |class| match class {
                BreakClass::After | BreakClass::Hyphen | BreakClass::NonStarter => true,
                _ => false,
            },
            Operator::Never,
        );
        transition_after(self, |class| class == BreakClass::Before, Operator::Never);

        // LB21a
        self.pairs(|a, mut b| {
            if let BreakClass::HebrewLetter = a.class {
                if let BreakClass::Hyphen | BreakClass::After = b.class {
                    b.transition(Operator::Never);
                }
            }
        });

        // LB21b
        transition_between(
            self,
            |a, b| (BreakClass::Symbol, BreakClass::HebrewLetter) == (a, b),
            Operator::Never,
        );

        // LB22
        transition_between(
            self,
            |a, b| match a {
                BreakClass::Alphabetic
                | BreakClass::HebrewLetter
                | BreakClass::Exclamation
                | BreakClass::Ideographic
                | BreakClass::EmojiBase
                | BreakClass::EmojiModifier
                | BreakClass::Numeric => b == BreakClass::Inseparable,
                _ => false,
            },
            Operator::Never,
        );

        if !T::breaks_between_inseperable() {
            transition_between(
                self,
                |a, b| (BreakClass::Inseparable, BreakClass::Inseparable) == (a, b),
                Operator::Never,
            );
        }

        // LB23 + LB23a
        transition_between(
            self,
            |a, b| match a {
                BreakClass::Alphabetic | BreakClass::HebrewLetter => b == BreakClass::Numeric,
                BreakClass::Numeric => match b {
                    BreakClass::Alphabetic | BreakClass::HebrewLetter => true,
                    _ => false,
                },
                BreakClass::Prefix => match b {
                    BreakClass::Ideographic | BreakClass::EmojiBase | BreakClass::EmojiModifier => {
                        true
                    }
                    _ => false,
                },
                BreakClass::Ideographic | BreakClass::EmojiBase | BreakClass::EmojiModifier => {
                    b == BreakClass::Postfix
                }
                _ => false,
            },
            Operator::Never,
        );

        // LB24
        transition_between(
            self,
            |a, b| match a {
                BreakClass::Prefix | BreakClass::Postfix => match b {
                    BreakClass::Alphabetic | BreakClass::HebrewLetter => true,
                    _ => false,
                },
                BreakClass::Alphabetic | BreakClass::HebrewLetter => match b {
                    BreakClass::Prefix | BreakClass::Postfix => true,
                    _ => false,
                },
                _ => false,
            },
            Operator::Never,
        );

        // LB25
        let len = self.len();
        for i in 0..len {
            let mut chars = self.classes.iter().enumerate().skip(i);
            enum State {
                Start,
                PrPo,
                OpHy,
                Nu,
                ClCp,
            }
            let mut state = State::Start;
            let end_state = loop {
                let (index, class) = match chars.next() {
                    Some(v) => v,
                    _ => {
                        break match state {
                            State::Nu | State::ClCp | State::PrPo => Some(len - 1),
                            _ => None,
                        };
                    }
                };
                state = match state {
                    State::Start => match class {
                        BreakClass::Prefix | BreakClass::Postfix => State::PrPo,
                        BreakClass::OpenPunctuation | BreakClass::Hyphen => State::OpHy,
                        BreakClass::Numeric => State::Nu,
                        _ => break None,
                    },
                    State::PrPo => match class {
                        BreakClass::OpenPunctuation | BreakClass::Hyphen => State::OpHy,
                        BreakClass::Numeric => State::Nu,
                        _ => break None,
                    },
                    State::OpHy => match class {
                        BreakClass::Numeric => State::Nu,
                        _ => break None,
                    },
                    State::Nu => match class {
                        BreakClass::Numeric | BreakClass::Symbol | BreakClass::InfixSeparator => {
                            continue;
                        }
                        BreakClass::ClosePunctuation | BreakClass::CloseParenthesis => State::ClCp,
                        BreakClass::Prefix | BreakClass::Postfix => break Some(index),
                        _ => break Some(index - 1),
                    },
                    State::ClCp => match class {
                        BreakClass::Prefix | BreakClass::Postfix => break Some(index),
                        _ => break Some(index - 1),
                    },
                };
            };
            if let Some(end_index) = end_state {
                for c in &mut self.operators[i..end_index] {
                    c.transition(Operator::Never);
                }
            }
        }

        // LB26
        transition_between(
            self,
            |a, b| match b {
                BreakClass::HangulLJamo
                | BreakClass::HangulVJamo
                | BreakClass::HangulLvSyllable
                | BreakClass::HangulLvtSyllable => a == BreakClass::HangulLJamo,
                _ => false,
            },
            Operator::Never,
        );
        transition_between(
            self,
            |a, b| match a {
                BreakClass::HangulVJamo | BreakClass::HangulLvSyllable => match b {
                    BreakClass::HangulVJamo | BreakClass::HangulTJamo => true,
                    _ => false,
                },
                BreakClass::HangulTJamo | BreakClass::HangulLvtSyllable => {
                    b == BreakClass::HangulTJamo
                }
                _ => false,
            },
            Operator::Never,
        );

        // LB27
        transition_between(
            self,
            |a, b| match a {
                BreakClass::HangulLJamo
                | BreakClass::HangulVJamo
                | BreakClass::HangulTJamo
                | BreakClass::HangulLvSyllable
                | BreakClass::HangulLvtSyllable => match b {
                    BreakClass::Inseparable | BreakClass::Postfix => true,
                    _ => false,
                },
                _ => false,
            },
            Operator::Never,
        );
        transition_between(
            self,
            |a, b| match b {
                BreakClass::HangulLJamo
                | BreakClass::HangulVJamo
                | BreakClass::HangulTJamo
                | BreakClass::HangulLvSyllable
                | BreakClass::HangulLvtSyllable => a == BreakClass::Prefix,
                _ => false,
            },
            Operator::Never,
        );

        // LB28 + LB29
        transition_between(
            self,
            |a, b| match a {
                BreakClass::Alphabetic | BreakClass::HebrewLetter | BreakClass::InfixSeparator => {
                    match b {
                        BreakClass::Alphabetic | BreakClass::HebrewLetter => true,
                        _ => false,
                    }
                }
                _ => false,
            },
            Operator::Never,
        );

        // LB30
        transition_between(
            self,
            |a, b| match a {
                BreakClass::Alphabetic | BreakClass::HebrewLetter | BreakClass::Numeric => {
                    b == BreakClass::OpenPunctuation
                }
                _ => false,
            },
            Operator::Never,
        );
        transition_between(
            self,
            |a, b| match b {
                BreakClass::Alphabetic | BreakClass::HebrewLetter | BreakClass::Numeric => {
                    a == BreakClass::CloseParenthesis
                }
                _ => false,
            },
            Operator::Never,
        );

        // LB30b
        transition_between(
            self,
            |a, b| (BreakClass::EmojiBase, BreakClass::EmojiModifier) == (a, b),
            Operator::Never,
        );

        // LB31
        transition_after(self, |_| true, Operator::Could);
    }

    pub fn split_str_iter<'a>(
        &'a self,
        text: &'a str,
    ) -> impl Iterator<Item = (BreakOpportunity, &'a str)> {
        let mut last_index = 0;
        self.operators
            .iter()
            .zip(text.char_indices())
            .filter_map(move |(o, (i, c))| match o {
                Operator::Could | Operator::Must => {
                    let end_index = i + c.len_utf8();
                    let start_index = mem::replace(&mut last_index, end_index);
                    Some((
                        if *o == Operator::Must {
                            BreakOpportunity::Must
                        } else {
                            BreakOpportunity::Could
                        },
                        &text[start_index..end_index],
                    ))
                }
                _ => None,
            })
    }

    pub fn into_split_str_iter<'a>(
        self,
        text: &'a str,
    ) -> impl Iterator<Item = (BreakOpportunity, &'a str)> {
        let mut last_index = 0;
        self.operators
            .into_iter()
            .zip(text.char_indices())
            .filter_map(move |(o, (i, c))| match o {
                Operator::Could | Operator::Must => {
                    let end_index = i + c.len_utf8();
                    let start_index = mem::replace(&mut last_index, end_index);
                    Some((
                        if o == Operator::Must {
                            BreakOpportunity::Must
                        } else {
                            BreakOpportunity::Could
                        },
                        &text[start_index..end_index],
                    ))
                }
                _ => None,
            })
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = Annotated<'_>> {
        self.operators
            .iter_mut()
            .zip(&mut self.classes)
            .map(|(operator, class)| Annotated { operator, class })
    }

    fn len(&self) -> usize {
        self.operators.len()
    }

    fn pairs(&mut self, mut f: impl FnMut(Annotated, Annotated)) {
        let mut iter = self.iter_mut();
        if let Some(mut first) = iter.next() {
            for second in iter {
                f(
                    first,
                    Annotated {
                        class: second.class,
                        operator: second.operator,
                    },
                );
                first = second;
            }
        }
    }
}

impl Operator {
    fn transition(&mut self, new_operator: Operator) {
        if *self == Operator::Unknown {
            *self = new_operator;
        }
    }
}

struct Annotated<'a> {
    operator: &'a mut Operator,
    class: &'a mut BreakClass,
}

impl<'a> Annotated<'a> {
    fn transition(&mut self, new_operator: Operator) {
        self.operator.transition(new_operator)
    }
}

fn transition_after_last_space(
    chars: &mut AnnotatedVec,
    mut start_at: impl FnMut(BreakClass) -> bool,
    operator: Operator,
) {
    for i in 0..chars.len() - 1 {
        if start_at(chars.classes[i]) {
            if let Some(mut last_space) = chars
                .iter_mut()
                .skip(i + 1)
                .take_while(|c| *c.class == BreakClass::Space)
                .last()
            {
                last_space.transition(operator);
            } else {
                chars.operators[i].transition(operator);
            }
        }
    }
}

fn transition_after_last_space_if(
    chars: &mut AnnotatedVec,
    mut start_at: impl FnMut(BreakClass) -> bool,
    mut ends_with: impl FnMut(BreakClass) -> bool,
    operator: Operator,
) {
    for i in 0..chars.len() - 1 {
        if start_at(chars.classes[i]) {
            if let Some((end_index, end)) = chars
                .classes
                .iter()
                .enumerate()
                .skip(i + 1)
                .skip_while(|(_, c)| **c == BreakClass::Space)
                .next()
            {
                if ends_with(*end) {
                    chars.operators[end_index - 1].transition(operator);
                }
            }
        }
    }
}

fn transition_before(
    chars: &mut AnnotatedVec,
    mut filter: impl FnMut(BreakClass) -> bool,
    operator: Operator,
) {
    chars.pairs(|mut a, b| {
        if filter(*b.class) {
            a.transition(operator);
        }
    });
}

fn transition_after(
    chars: &mut AnnotatedVec,
    mut filter: impl FnMut(BreakClass) -> bool,
    operator: Operator,
) {
    chars
        .iter_mut()
        .filter(|c| filter(*c.class))
        .for_each(|mut c| c.transition(operator));
}

fn transition_around(
    chars: &mut AnnotatedVec,
    mut filter: impl FnMut(BreakClass) -> bool,
    operator: Operator,
) {
    if let Some(mut first) = chars.iter_mut().next() {
        if filter(*first.class) {
            first.transition(operator);
        }
    }
    chars.pairs(|mut a, mut b| {
        if filter(*b.class) {
            a.transition(operator);
            b.transition(operator);
        }
    });
}

fn transition_between(
    chars: &mut AnnotatedVec,
    mut filter: impl FnMut(BreakClass, BreakClass) -> bool,
    operator: Operator,
) {
    chars.pairs(|mut a, b| {
        if filter(*a.class, *b.class) {
            a.transition(operator);
        }
    });
}

pub fn break_lines<T: Tailoring>(
    text: &str,
    tailoring: T,
) -> impl Iterator<Item = (BreakOpportunity, &str)> {
    let mut vec = AnnotatedVec::new();
    vec.extend_str(text, tailoring);
    vec.into_split_str_iter(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let chunks = break_lines("Hello, World!\r\nNew line here", Strict).collect::<Vec<_>>();

        assert_eq!(
            chunks,
            [
                (BreakOpportunity::Could, "Hello, "),
                (BreakOpportunity::Must, "World!\r\n"),
                (BreakOpportunity::Could, "New "),
                (BreakOpportunity::Could, "line "),
                (BreakOpportunity::Must, "here"),
            ],
        );
    }

    #[test]
    fn test_file() {
        let test_file = include_str!("../LineBreakTest.txt");
        let mut test_text = String::new();
        let mut operators = Vec::new();
        let mut annotated = AnnotatedVec::new();
        for line in test_file.lines() {
            let mut splits = line.splitn(2, "#");
            if let (Some(rule), Some(comment)) = (splits.next(), splits.next()) {
                if rule.is_empty() {
                    continue;
                }

                let mut splits = rule["× ".len()..].split_whitespace();
                test_text.clear();
                while let (Some(codepoint), Some(operator)) = (splits.next(), splits.next()) {
                    let codepoint =
                        std::char::from_u32(u32::from_str_radix(codepoint, 16).unwrap()).unwrap();
                    test_text.push(codepoint);
                    operators.push((operator == "×", test_text.len()));
                }

                annotated.clear();
                annotated.extend_str(&test_text, Strict);

                for (expected, actual_operator) in operators.drain(..).zip(&annotated.operators) {
                    assert_eq!(
                        *actual_operator == Operator::Never,
                        expected.0,
                        "Failed {:?}: {}\n{:#?}",
                        test_text,
                        comment.trim_start(),
                        annotated,
                    );
                }
            }
        }
    }
}
