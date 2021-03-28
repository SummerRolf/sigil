use combine::error::ParseError;
use combine::parser;
use combine::parser::{
    char::alpha_num, char::char, char::digit, char::newline, char::space, choice::choice,
    choice::optional, choice::or, combinator::from_str, range::range, range::recognize,
    repeat::many, repeat::skip_many1, repeat::skip_until, sequence::between, token::eof,
    token::one_of,
};
use combine::stream::position::Stream;
use combine::{EasyParser, Parser, RangeStream};
use std::fmt;
use thiserror::Error;

#[derive(Debug, PartialEq)]
pub enum Form<'a> {
    Nil,
    Bool(bool),
    Number(u64),
    String(&'a str),
    // identifier, optional namespace
    Keyword(&'a str, Option<&'a str>),
    // identifier, optional namespace
    Symbol(&'a str, Option<&'a str>),
    Comment(&'a str),
    Whitespace(&'a str),
    List(Vec<Form<'a>>),
    Vector(Vec<Form<'a>>),
    Map(Vec<Form<'a>>),
    Set(Vec<Form<'a>>),
}

impl fmt::Display for Form<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Form::*;

        match self {
            Nil => write!(f, "nil"),
            Bool(b) => write!(f, "{}", b),
            Number(n) => write!(f, "{}", n),
            String(s) => write!(f, "\"{}\"", s),
            Keyword(id, ns_opt) => {
                write!(f, ":")?;
                if let Some(ns) = ns_opt {
                    write!(f, "{}/", ns)?;
                }
                write!(f, "{}", id)
            }
            Symbol(id, ns_opt) => {
                if let Some(ns) = ns_opt {
                    write!(f, "{}/", ns)?;
                }
                write!(f, "{}", id)
            }
            Comment(s) => write!(f, ";{}", s),
            Whitespace(s) => write!(f, "{}", s),
            List(elems) => {
                write!(f, "(")?;
                for elem in elems {
                    write!(f, "{}", elem)?;
                }
                write!(f, ")")
            }
            Vector(elems) => {
                write!(f, "[")?;
                for elem in elems {
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            Map(elems) => {
                write!(f, "{{")?;
                for elem in elems {
                    write!(f, "{}", elem)?;
                }
                write!(f, "}}")
            }
            Set(elems) => {
                write!(f, "#{{")?;
                for elem in elems {
                    write!(f, "{}", elem)?;
                }
                write!(f, "}}")
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("reader could not parse the full input")]
    ExtraneousInput,
    #[error("{0}")]
    ParserError(String),
    #[error("unknown error occurred")]
    Unknown,
}

#[inline]
fn read_forms<'a, Input>() -> impl Parser<Input, Output = Vec<Form<'a>>>
where
    Input: RangeStream<Token = char, Range = &'a str> + 'a,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    read_forms_inner()
}

parser! {
    #[inline]
    fn read_forms_inner['a, Input]()(Input) -> Vec<Form<'a>>
    where [ Input: RangeStream<Token = char, Range = &'a str> + 'a]
    {
        let number = from_str(recognize(skip_many1(digit()))).map(Form::Number);
        let nil = recognize(range("nil")).map(|_| Form::Nil);
        let true_parser = recognize(range("true")).map(|_| Form::Bool(true));
        let false_parser = recognize(range("false")).map(|_| Form::Bool(false));
        let whitespace = recognize(skip_many1(or(space(), char(',')))).map(Form::Whitespace);

        let string =
            (char('"'), recognize(skip_until(char('"'))), char('"')).map(|(_, s, _)| Form::String(s));
        let identifier_tokens = || alpha_num().or(one_of("*+!-_'?<>=".chars()));
        let identifier_with_namespace = || (recognize(skip_many1(identifier_tokens())), optional((char('/'), recognize(skip_many1(identifier_tokens()))))).map(|(first, rest)| {
            if let Some((_, rest)) = rest {
                (rest, Some(first))
            } else {
                (first, None)
            }
        });
        let symbol = identifier_with_namespace().map(|(id, ns)| Form::Symbol(id, ns));
        let keyword = (char(':'), identifier_with_namespace()).map(|(_, (id, ns))| Form::Keyword(id, ns));
        let comment = (char(';'), recognize(skip_until(or(eof(), newline().map(|_| ())))), optional(newline())).map(|(_, s, _)| Form::Comment(s));
        let list = between(char('('), char(')'), read_forms()).map(Form::List);
        let vector = between(char('['), char(']'), read_forms()).map(Form::Vector);
        let map_elems = || between(char('{'), char('}'), read_forms());
        let map = map_elems().map(Form::Map);
        let set = (char('#'), map_elems()).map(|(_, elems)| {
            Form::Set(elems)
        });

        let forms = choice((
            whitespace,
            number,
            nil,
            true_parser,
            false_parser,
            string,
            keyword,
            symbol,
            comment,
            list,
            vector,
            map,
            set,
        ));

        many::<Vec<_>, _, _>(forms)
    }
}

pub fn read(input: &str) -> Result<Vec<Form>, ReaderError> {
    let mut parser = (read_forms(), eof()).map(|(result, _)| result);
    let parse_result = parser.easy_parse(Stream::new(input));
    match parse_result {
        Ok((forms, _)) => Ok(forms),
        Err(error) => Err(ReaderError::ParserError(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::{read, Form::*};

    #[test]
    fn test_read_basic() {
        let cases = vec![
            ("1337", vec![Number(1337)]),
            ("nil", vec![Nil]),
            ("true", vec![Bool(true)]),
            ("false", vec![Bool(false)]),
            (",,,", vec![Whitespace(",,,")]),
            ("  ", vec![Whitespace("  ")]),
            (" , ", vec![Whitespace(" , ")]),
            (
                "true, , , false",
                vec![Bool(true), Whitespace(", , , "), Bool(false)],
            ),
            ("", vec![]),
            ("baz", vec![Symbol("baz", None)]),
            ("foo/baz", vec![Symbol("baz", Some("foo"))]),
            (
                "foo/baz bar true",
                vec![
                    Symbol("baz", Some("foo")),
                    Whitespace(" "),
                    Symbol("bar", None),
                    Whitespace(" "),
                    Bool(true),
                ],
            ),
            ("\"\"", vec![String("")]),
            ("\"hi\"", vec![String("hi")]),
            (
                "\"123foo\" true",
                vec![String("123foo"), Whitespace(" "), Bool(true)],
            ),
            (r#""abc""#, vec![String("abc")]),
            (":foobar", vec![Keyword("foobar", None)]),
            (":net/hi", vec![Keyword("hi", Some("net"))]),
            (":a0987234", vec![Keyword("a0987234", None)]),
            (
                "; sdlkfjsldfjsldjflsdjf",
                vec![Comment(" sdlkfjsldfjsldjflsdjf")],
            ),
            (
                "foo/bar true ;; some comment",
                vec![
                    Symbol("bar", Some("foo")),
                    Whitespace(" "),
                    Bool(true),
                    Whitespace(" "),
                    Comment("; some comment"),
                ],
            ),
            (
                "baz ;; comment \nfoo12",
                vec![
                    Symbol("baz", None),
                    Whitespace(" "),
                    Comment("; comment "),
                    Symbol("foo12", None),
                ],
            ),
            ("()", vec![List(vec![])]),
            (
                "(a b c)",
                vec![List(vec![
                    Symbol("a", None),
                    Whitespace(" "),
                    Symbol("b", None),
                    Whitespace(" "),
                    Symbol("c", None),
                ])],
            ),
            (
                "(12 :foo/bar \"extra\")",
                vec![List(vec![
                    Number(12),
                    Whitespace(" "),
                    Keyword("bar", Some("foo")),
                    Whitespace(" "),
                    String("extra"),
                ])],
            ),
            ("[]", vec![Vector(vec![])]),
            (
                "[a b c]",
                vec![Vector(vec![
                    Symbol("a", None),
                    Whitespace(" "),
                    Symbol("b", None),
                    Whitespace(" "),
                    Symbol("c", None),
                ])],
            ),
            (
                "[12 :foo/bar \"extra\"]",
                vec![Vector(vec![
                    Number(12),
                    Whitespace(" "),
                    Keyword("bar", Some("foo")),
                    Whitespace(" "),
                    String("extra"),
                ])],
            ),
            ("{}", vec![Map(vec![])]),
            (
                "{a b c}",
                vec![Map(vec![
                    Symbol("a", None),
                    Whitespace(" "),
                    Symbol("b", None),
                    Whitespace(" "),
                    Symbol("c", None),
                ])],
            ),
            (
                "{12 :foo/bar \"extra\"}",
                vec![Map(vec![
                    Number(12),
                    Whitespace(" "),
                    Keyword("bar", Some("foo")),
                    Whitespace(" "),
                    String("extra"),
                ])],
            ),
            ("() []", vec![List(vec![]), Whitespace(" "), Vector(vec![])]),
            ("(())", vec![List(vec![List(vec![])])]),
            ("(([]))", vec![List(vec![List(vec![Vector(vec![])])])]),
            (
                "(([]))()",
                vec![List(vec![List(vec![Vector(vec![])])]), List(vec![])],
            ),
            (
                "(12 (true [34 false]))(), (7)",
                vec![
                    List(vec![
                        Number(12),
                        Whitespace(" "),
                        List(vec![
                            Bool(true),
                            Whitespace(" "),
                            Vector(vec![Number(34), Whitespace(" "), Bool(false)]),
                        ]),
                    ]),
                    List(vec![]),
                    Whitespace(", "),
                    List(vec![Number(7)]),
                ],
            ),
            ("#{}", vec![Set(vec![])]),
            (
                "#{1 2 3}",
                vec![Set(vec![
                    Number(1),
                    Whitespace(" "),
                    Number(2),
                    Whitespace(" "),
                    Number(3),
                ])],
            ),
            (
                "#{(1 3) :foo}",
                vec![Set(vec![
                    List(vec![Number(1), Whitespace(" "), Number(3)]),
                    Whitespace(" "),
                    Keyword("foo", None),
                ])],
            ),
            (
                "+ ! =",
                vec![
                    Symbol("+", None),
                    Whitespace(" "),
                    Symbol("!", None),
                    Whitespace(" "),
                    Symbol("=", None),
                ],
            ),
            (
                "(defn foo [a] (+ a 1))",
                vec![List(vec![
                    Symbol("defn", None),
                    Whitespace(" "),
                    Symbol("foo", None),
                    Whitespace(" "),
                    Vector(vec![Symbol("a", None)]),
                    Whitespace(" "),
                    List(vec![
                        Symbol("+", None),
                        Whitespace(" "),
                        Symbol("a", None),
                        Whitespace(" "),
                        Number(1),
                    ]),
                ])],
            ),
        ];
        for (input, expected) in cases {
            match read(input) {
                Ok(result) => assert_eq!(result, expected),
                Err(e) => {
                    dbg!(e);
                    panic!();
                }
            }
        }
    }
}
