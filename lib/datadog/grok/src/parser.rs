// auto-generated: "lalrpop 0.19.6"
// sha3: d4dc7affd87d19338f3f2faa8b89ada4699c6dd58a589788984647392c27
use crate::ast::*;
use crate::lexer::*;
use parsing::value::Value;
use lookup::{LookupBuf, SegmentBuf, FieldBuf};
#[allow(unused_extern_crates)]
extern crate lalrpop_util as __lalrpop_util;
#[allow(unused_imports)]
use self::__lalrpop_util::state_machine as __state_machine;
extern crate core;
extern crate alloc;

#[cfg_attr(rustfmt, rustfmt_skip)]
mod __parse__GrokFilter {
    #![allow(non_snake_case, non_camel_case_types, unused_mut, unused_variables, unused_imports, unused_parens)]

    use crate::ast::*;
    use crate::lexer::*;
    use parsing::value::Value;
    use lookup::{LookupBuf, SegmentBuf, FieldBuf};
    #[allow(unused_extern_crates)]
    extern crate lalrpop_util as __lalrpop_util;
    #[allow(unused_imports)]
    use self::__lalrpop_util::state_machine as __state_machine;
    extern crate core;
    extern crate alloc;
    use super::__ToTriple;
    #[allow(dead_code)]
    pub(crate) enum __Symbol<'input>
     {
        Variant0(Tok<'input>),
        Variant1(&'input str),
        Variant2(f64),
        Variant3(i64),
        Variant4(StringLiteral<&'input str>),
        Variant5(core::option::Option<Tok<'input>>),
        Variant6((Tok<'input>, &'input str)),
        Variant7(alloc::vec::Vec<(Tok<'input>, &'input str)>),
        Variant8(FunctionArgument),
        Variant9(alloc::vec::Vec<FunctionArgument>),
        Variant10(Destination),
        Variant11(core::option::Option<Destination>),
        Variant12(Function),
        Variant13(core::option::Option<Function>),
        Variant14(core::option::Option<FunctionArgument>),
        Variant15(Vec<FunctionArgument>),
        Variant16(core::option::Option<Vec<FunctionArgument>>),
        Variant17(bool),
        Variant18(FieldBuf),
        Variant19(GrokPattern),
        Variant20(Value),
        Variant21(LookupBuf),
        Variant22(core::option::Option<LookupBuf>),
        Variant23(()),
        Variant24(SegmentBuf),
        Variant25(String),
    }
    const __ACTION: &[i8] = &[
        // State 0
        2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 1
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0,
        // State 2
        0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19,
        // State 3
        0, -61, -61, -61, 7, -61, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -61,
        // State 4
        0, 8, -43, -43, 0, -43, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -43,
        // State 5
        0, 0, 0, 0, 11, 0, 12, 0, 26, 0, 0, 17, 0, 0, 0, 0, 0,
        // State 6
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0,
        // State 7
        0, 0, 36, 0, 0, 0, 0, 0, 0, 37, 38, 17, 39, 40, 41, 42, 0,
        // State 8
        0, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -36,
        // State 9
        0, 0, 0, 0, 11, -54, 12, 0, 26, 0, 0, 17, 0, 0, 0, 0, -54,
        // State 10
        0, 0, 0, 0, 0, 0, 0, 0, 26, 0, 0, 17, 0, 0, 0, 0, 0,
        // State 11
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 41, 0, 0,
        // State 12
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0,
        // State 13
        0, 0, 49, 0, 0, 0, 0, 0, 0, 37, 38, 17, 39, 40, 41, 42, 0,
        // State 14
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0,
        // State 15
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 16
        0, -46, -46, -46, -46, -46, -46, 0, -46, 0, 0, -46, 0, 0, 0, 0, -46,
        // State 17
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22,
        // State 18
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 19
        0, -62, -62, -62, 13, -62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -62,
        // State 20
        0, 0, -42, -42, 0, -42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -42,
        // State 21
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 22
        0, 0, 0, 0, -39, -39, -39, 0, -39, 0, 0, -39, 0, 0, 0, 0, -39,
        // State 23
        0, 0, 0, 0, -59, -59, -59, 0, -59, 0, 0, -59, 0, 0, 0, 0, -59,
        // State 24
        0, 0, 0, 0, -38, -38, -38, 0, -38, 0, 0, -38, 0, 0, 0, 0, -38,
        // State 25
        0, 0, 0, 0, -37, -37, -37, 0, -37, 0, 0, -37, 0, 0, 0, 0, -37,
        // State 26
        0, -6, -6, -6, -6, -6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -6,
        // State 27
        0, 0, 50, 51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 28
        0, 0, -51, -51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 29
        0, 0, -49, -49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 30
        0, 0, -20, -20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 31
        0, 0, -48, -48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 32
        0, 0, -19, -19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 33
        0, 0, -52, -52, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 34
        0, 0, -50, -50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 35
        0, 0, -24, -24, 0, -24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -24,
        // State 36
        0, 0, -30, -30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 37
        0, 0, -41, -41, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 38
        0, 0, -47, -47, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 39
        0, 0, -57, -57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 40
        0, 0, -63, -63, 0, 0, 0, -63, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 41
        0, 0, -29, -29, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 42
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -35,
        // State 43
        0, 0, 0, 0, 0, -53, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -53,
        // State 44
        0, 0, 0, 0, -58, -58, -58, 0, -58, 0, 0, -58, 0, 0, 0, 0, -58,
        // State 45
        0, 0, 0, 0, 0, 0, 0, 53, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 46
        0, -7, -7, -7, -7, -7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -7,
        // State 47
        0, 0, 54, 55, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 48
        0, 0, -26, -26, 0, -26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -26,
        // State 49
        0, 0, -23, -23, 0, -23, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -23,
        // State 50
        0, 0, -11, 0, 0, 0, 0, 0, 0, -11, -11, -11, -11, -11, -11, -11, 0,
        // State 51
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -40,
        // State 52
        0, 0, 0, 0, -60, -60, -60, 0, -60, 0, 0, -60, 0, 0, 0, 0, -60,
        // State 53
        0, 0, -25, -25, 0, -25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -25,
        // State 54
        0, 0, -12, 0, 0, 0, 0, 0, 0, -12, -12, -12, -12, -12, -12, -12, 0,
    ];
    fn __action(state: i8, integer: usize) -> i8 {
        __ACTION[(state as usize) * 17 + integer]
    }
    const __EOF_ACTION: &[i8] = &[
        // State 0
        0,
        // State 1
        0,
        // State 2
        0,
        // State 3
        0,
        // State 4
        0,
        // State 5
        0,
        // State 6
        0,
        // State 7
        0,
        // State 8
        0,
        // State 9
        0,
        // State 10
        0,
        // State 11
        0,
        // State 12
        0,
        // State 13
        0,
        // State 14
        0,
        // State 15
        -64,
        // State 16
        0,
        // State 17
        0,
        // State 18
        -45,
        // State 19
        0,
        // State 20
        0,
        // State 21
        -44,
        // State 22
        0,
        // State 23
        0,
        // State 24
        0,
        // State 25
        0,
        // State 26
        0,
        // State 27
        0,
        // State 28
        0,
        // State 29
        0,
        // State 30
        0,
        // State 31
        0,
        // State 32
        0,
        // State 33
        0,
        // State 34
        0,
        // State 35
        0,
        // State 36
        0,
        // State 37
        0,
        // State 38
        0,
        // State 39
        0,
        // State 40
        0,
        // State 41
        0,
        // State 42
        0,
        // State 43
        0,
        // State 44
        0,
        // State 45
        0,
        // State 46
        0,
        // State 47
        0,
        // State 48
        0,
        // State 49
        0,
        // State 50
        0,
        // State 51
        0,
        // State 52
        0,
        // State 53
        0,
        // State 54
        0,
    ];
    fn __goto(state: i8, nt: usize) -> i8 {
        match nt {
            3 => 19,
            6 => 13,
            11 => match state {
                13 => 47,
                _ => 27,
            },
            13 => 20,
            15 => 28,
            17 => 17,
            18 => 22,
            19 => match state {
                10 => 44,
                _ => 23,
            },
            20 => 42,
            21 => 29,
            22 => match state {
                1 => 2,
                14 => 51,
                _ => 30,
            },
            23 => 15,
            24 => match state {
                5 | 9..=10 => 24,
                6 => 26,
                12 => 46,
                _ => 3,
            },
            25 => 31,
            26 => 32,
            27 => match state {
                9 => 43,
                _ => 8,
            },
            29 => 33,
            30 => 9,
            31 => 4,
            32 => match state {
                11 => 45,
                _ => 34,
            },
            _ => 0,
        }
    }
    fn __expected_tokens(__state: i8) -> alloc::vec::Vec<alloc::string::String> {
        const __TERMINAL: &[&str] = &[
            r###""%{""###,
            r###""(""###,
            r###"")""###,
            r###"",""###,
            r###"".""###,
            r###"":""###,
            r###""[""###,
            r###""]""###,
            r###""extended identifier""###,
            r###""false""###,
            r###""float literal""###,
            r###""identifier""###,
            r###""integer literal""###,
            r###""null""###,
            r###""string literal""###,
            r###""true""###,
            r###""}""###,
        ];
        __TERMINAL.iter().enumerate().filter_map(|(index, terminal)| {
            let next_state = __action(__state, index);
            if next_state == 0 {
                None
            } else {
                Some(alloc::string::ToString::to_string(terminal))
            }
        }).collect()
    }
    pub(crate) struct __StateMachine<'err, 'input>
    where 
    {
        input: &'input str,
        __phantom: core::marker::PhantomData<(&'err (), &'input ())>,
    }
    impl<'err, 'input> __state_machine::ParserDefinition for __StateMachine<'err, 'input>
    where 
    {
        type Location = usize;
        type Error = Error;
        type Token = Tok<'input>;
        type TokenIndex = usize;
        type Symbol = __Symbol<'input>;
        type Success = GrokPattern;
        type StateIndex = i8;
        type Action = i8;
        type ReduceIndex = i8;
        type NonterminalIndex = usize;

        #[inline]
        fn start_location(&self) -> Self::Location {
              Default::default()
        }

        #[inline]
        fn start_state(&self) -> Self::StateIndex {
              0
        }

        #[inline]
        fn token_to_index(&self, token: &Self::Token) -> Option<usize> {
            __token_to_integer(token, core::marker::PhantomData::<(&(), &())>)
        }

        #[inline]
        fn action(&self, state: i8, integer: usize) -> i8 {
            __action(state, integer)
        }

        #[inline]
        fn error_action(&self, state: i8) -> i8 {
            __action(state, 17 - 1)
        }

        #[inline]
        fn eof_action(&self, state: i8) -> i8 {
            __EOF_ACTION[state as usize]
        }

        #[inline]
        fn goto(&self, state: i8, nt: usize) -> i8 {
            __goto(state, nt)
        }

        fn token_to_symbol(&self, token_index: usize, token: Self::Token) -> Self::Symbol {
            __token_to_symbol(token_index, token, core::marker::PhantomData::<(&(), &())>)
        }

        fn expected_tokens(&self, state: i8) -> alloc::vec::Vec<alloc::string::String> {
            __expected_tokens(state)
        }

        #[inline]
        fn uses_error_recovery(&self) -> bool {
            false
        }

        #[inline]
        fn error_recovery_symbol(
            &self,
            recovery: __state_machine::ErrorRecovery<Self>,
        ) -> Self::Symbol {
            panic!("error recovery not enabled for this grammar")
        }

        fn reduce(
            &mut self,
            action: i8,
            start_location: Option<&Self::Location>,
            states: &mut alloc::vec::Vec<i8>,
            symbols: &mut alloc::vec::Vec<__state_machine::SymbolTriple<Self>>,
        ) -> Option<__state_machine::ParseResult<Self>> {
            __reduce(
                self.input,
                action,
                start_location,
                states,
                symbols,
                core::marker::PhantomData::<(&(), &())>,
            )
        }

        fn simulate_reduce(&self, action: i8) -> __state_machine::SimulatedReduce<Self> {
            panic!("error recovery not enabled for this grammar")
        }
    }
    fn __token_to_integer<
        'err,
        'input,
    >(
        __token: &Tok<'input>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> Option<usize>
    {
        match *__token {
            Token::LRule if true => Some(0),
            Token::LParen if true => Some(1),
            Token::RParen if true => Some(2),
            Token::Comma if true => Some(3),
            Token::Dot if true => Some(4),
            Token::Colon if true => Some(5),
            Token::LBracket if true => Some(6),
            Token::RBracket if true => Some(7),
            Token::ExtendedIdentifier(_) if true => Some(8),
            Token::False if true => Some(9),
            Token::FloatLiteral(_) if true => Some(10),
            Token::Identifier(_) if true => Some(11),
            Token::IntegerLiteral(_) if true => Some(12),
            Token::Null if true => Some(13),
            Token::StringLiteral(_) if true => Some(14),
            Token::True if true => Some(15),
            Token::RRule if true => Some(16),
            _ => None,
        }
    }
    fn __token_to_symbol<
        'err,
        'input,
    >(
        __token_index: usize,
        __token: Tok<'input>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> __Symbol<'input>
    {
        match __token_index {
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 9 | 13 | 15 | 16 => __Symbol::Variant0(__token),
            8 | 11 => match __token {
                Token::ExtendedIdentifier(__tok0) | Token::Identifier(__tok0) if true => __Symbol::Variant1(__tok0),
                _ => unreachable!(),
            },
            10 => match __token {
                Token::FloatLiteral(__tok0) if true => __Symbol::Variant2(__tok0),
                _ => unreachable!(),
            },
            12 => match __token {
                Token::IntegerLiteral(__tok0) if true => __Symbol::Variant3(__tok0),
                _ => unreachable!(),
            },
            14 => match __token {
                Token::StringLiteral(__tok0) if true => __Symbol::Variant4(__tok0),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
    pub struct GrokFilterParser {
        _priv: (),
    }

    impl GrokFilterParser {
        pub fn new() -> GrokFilterParser {
            GrokFilterParser {
                _priv: (),
            }
        }

        #[allow(dead_code)]
        pub fn parse<
            'err,
            'input,
            __TOKEN: __ToTriple<'err, 'input, >,
            __TOKENS: IntoIterator<Item=__TOKEN>,
        >(
            &self,
            input: &'input str,
            __tokens0: __TOKENS,
        ) -> Result<GrokPattern, __lalrpop_util::ParseError<usize, Tok<'input>, Error>>
        {
            let __tokens = __tokens0.into_iter();
            let mut __tokens = __tokens.map(|t| __ToTriple::to_triple(t));
            __state_machine::Parser::drive(
                __StateMachine {
                    input,
                    __phantom: core::marker::PhantomData::<(&(), &())>,
                },
                __tokens,
            )
        }
    }
    pub(crate) fn __reduce<
        'err,
        'input,
    >(
        input: &'input str,
        __action: i8,
        __lookahead_start: Option<&usize>,
        __states: &mut alloc::vec::Vec<i8>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> Option<Result<GrokPattern,__lalrpop_util::ParseError<usize, Tok<'input>, Error>>>
    {
        let (__pop_states, __nonterminal) = match __action {
            0 => {
                __reduce0(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            1 => {
                __reduce1(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            2 => {
                __reduce2(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            3 => {
                __reduce3(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            4 => {
                __reduce4(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            5 => {
                __reduce5(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            6 => {
                __reduce6(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            7 => {
                __reduce7(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            8 => {
                __reduce8(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            9 => {
                __reduce9(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            10 => {
                __reduce10(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            11 => {
                __reduce11(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            12 => {
                __reduce12(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            13 => {
                __reduce13(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            14 => {
                __reduce14(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            15 => {
                __reduce15(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            16 => {
                __reduce16(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            17 => {
                __reduce17(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            18 => {
                __reduce18(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            19 => {
                __reduce19(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            20 => {
                __reduce20(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            21 => {
                __reduce21(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            22 => {
                __reduce22(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            23 => {
                __reduce23(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            24 => {
                __reduce24(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            25 => {
                __reduce25(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            26 => {
                __reduce26(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            27 => {
                __reduce27(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            28 => {
                __reduce28(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            29 => {
                __reduce29(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            30 => {
                __reduce30(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            31 => {
                __reduce31(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            32 => {
                __reduce32(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            33 => {
                __reduce33(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            34 => {
                __reduce34(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            35 => {
                __reduce35(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            36 => {
                __reduce36(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            37 => {
                __reduce37(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            38 => {
                __reduce38(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            39 => {
                __reduce39(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            40 => {
                __reduce40(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            41 => {
                __reduce41(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            42 => {
                __reduce42(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            43 => {
                __reduce43(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            44 => {
                __reduce44(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            45 => {
                __reduce45(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            46 => {
                __reduce46(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            47 => {
                __reduce47(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            48 => {
                __reduce48(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            49 => {
                __reduce49(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            50 => {
                __reduce50(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            51 => {
                __reduce51(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            52 => {
                __reduce52(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            53 => {
                __reduce53(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            54 => {
                __reduce54(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            55 => {
                __reduce55(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            56 => {
                __reduce56(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            57 => {
                __reduce57(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            58 => {
                __reduce58(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            59 => {
                __reduce59(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            60 => {
                __reduce60(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            61 => {
                __reduce61(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            62 => {
                __reduce62(input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), &())>)
            }
            63 => {
                // __GrokFilter = GrokFilter => ActionFn(0);
                let __sym0 = __pop_Variant19(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = super::__action0::<>(input, __sym0);
                return Some(Ok(__nt));
            }
            _ => panic!("invalid action code {}", __action)
        };
        let __states_len = __states.len();
        __states.truncate(__states_len - __pop_states);
        let __state = *__states.last().unwrap();
        let __next_state = __goto(__state, __nonterminal);
        __states.push(__next_state);
        None
    }
    #[inline(never)]
    fn __symbol_type_mismatch() -> ! {
        panic!("symbol type mismatch")
    }
    fn __pop_Variant23<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, (), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant23(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant6<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, (Tok<'input>, &'input str), usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant6(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant10<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, Destination, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant10(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant18<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, FieldBuf, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant18(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant12<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, Function, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant12(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant8<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, FunctionArgument, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant8(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant19<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, GrokPattern, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant19(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant21<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, LookupBuf, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant21(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant24<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, SegmentBuf, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant24(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant25<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, String, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant25(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant4<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, StringLiteral<&'input str>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant4(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant0<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, Tok<'input>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant0(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant20<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, Value, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant20(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant15<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, Vec<FunctionArgument>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant15(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant7<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, alloc::vec::Vec<(Tok<'input>, &'input str)>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant7(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant9<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, alloc::vec::Vec<FunctionArgument>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant9(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant17<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, bool, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant17(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant11<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, core::option::Option<Destination>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant11(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant13<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, core::option::Option<Function>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant13(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant14<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, core::option::Option<FunctionArgument>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant14(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant22<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, core::option::Option<LookupBuf>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant22(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant5<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, core::option::Option<Tok<'input>>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant5(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant16<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, core::option::Option<Vec<FunctionArgument>>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant16(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant2<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, f64, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant2(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant3<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, i64, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant3(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant1<
      'input,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>
    ) -> (usize, &'input str, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant1(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    pub(crate) fn __reduce0<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // "."? = "." => ActionFn(33);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action33::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 0)
    }
    pub(crate) fn __reduce1<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // "."? =  => ActionFn(34);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action34::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (0, 0)
    }
    pub(crate) fn __reduce2<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ("." Identifier) = ".", Identifier => ActionFn(30);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action30::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (2, 1)
    }
    pub(crate) fn __reduce3<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ("." Identifier)* =  => ActionFn(28);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action28::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (0, 2)
    }
    pub(crate) fn __reduce4<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ("." Identifier)* = ("." Identifier)+ => ActionFn(29);
        let __sym0 = __pop_Variant7(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action29::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce5<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ("." Identifier)+ = ".", Identifier => ActionFn(54);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action54::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (2, 3)
    }
    pub(crate) fn __reduce6<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ("." Identifier)+ = ("." Identifier)+, ".", Identifier => ActionFn(55);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant1(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant7(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action55::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (3, 3)
    }
    pub(crate) fn __reduce7<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (<Arg> ",") = Arg, "," => ActionFn(49);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant8(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action49::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant8(__nt), __end));
        (2, 4)
    }
    pub(crate) fn __reduce8<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (<Arg> ",")* =  => ActionFn(47);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action47::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (0, 5)
    }
    pub(crate) fn __reduce9<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (<Arg> ",")* = (<Arg> ",")+ => ActionFn(48);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action48::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (1, 5)
    }
    pub(crate) fn __reduce10<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (<Arg> ",")+ = Arg, "," => ActionFn(58);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant8(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action58::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (2, 6)
    }
    pub(crate) fn __reduce11<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (<Arg> ",")+ = (<Arg> ",")+, Arg, "," => ActionFn(59);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant8(__symbols);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action59::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant9(__nt), __end));
        (3, 6)
    }
    pub(crate) fn __reduce12<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (DestinationAndFilter) = DestinationAndFilter => ActionFn(42);
        let __sym0 = __pop_Variant10(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action42::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant10(__nt), __end));
        (1, 7)
    }
    pub(crate) fn __reduce13<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (DestinationAndFilter)? = DestinationAndFilter => ActionFn(62);
        let __sym0 = __pop_Variant10(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action62::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant11(__nt), __end));
        (1, 8)
    }
    pub(crate) fn __reduce14<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (DestinationAndFilter)? =  => ActionFn(41);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action41::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant11(__nt), __end));
        (0, 8)
    }
    pub(crate) fn __reduce15<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (FilterFn) = FilterFn => ActionFn(39);
        let __sym0 = __pop_Variant12(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action39::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (1, 9)
    }
    pub(crate) fn __reduce16<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (FilterFn)? = FilterFn => ActionFn(65);
        let __sym0 = __pop_Variant12(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action65::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant13(__nt), __end));
        (1, 10)
    }
    pub(crate) fn __reduce17<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // (FilterFn)? =  => ActionFn(38);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action38::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant13(__nt), __end));
        (0, 10)
    }
    pub(crate) fn __reduce18<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Arg = Literal => ActionFn(12);
        let __sym0 = __pop_Variant20(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action12::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant8(__nt), __end));
        (1, 11)
    }
    pub(crate) fn __reduce19<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Arg = FunctionOrRef => ActionFn(13);
        let __sym0 = __pop_Variant12(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action13::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant8(__nt), __end));
        (1, 11)
    }
    pub(crate) fn __reduce20<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Arg? = Arg => ActionFn(45);
        let __sym0 = __pop_Variant8(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action45::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant14(__nt), __end));
        (1, 12)
    }
    pub(crate) fn __reduce21<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Arg? =  => ActionFn(46);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action46::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant14(__nt), __end));
        (0, 12)
    }
    pub(crate) fn __reduce22<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ArgsList = "(", Arg, ")" => ActionFn(74);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant8(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action74::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (3, 13)
    }
    pub(crate) fn __reduce23<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ArgsList = "(", ")" => ActionFn(75);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action75::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (2, 13)
    }
    pub(crate) fn __reduce24<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ArgsList = "(", (<Arg> ",")+, Arg, ")" => ActionFn(76);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant8(__symbols);
        let __sym1 = __pop_Variant9(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action76::<>(input, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (4, 13)
    }
    pub(crate) fn __reduce25<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ArgsList = "(", (<Arg> ",")+, ")" => ActionFn(77);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant9(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action77::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (3, 13)
    }
    pub(crate) fn __reduce26<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ArgsList? = ArgsList => ActionFn(31);
        let __sym0 = __pop_Variant15(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action31::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant16(__nt), __end));
        (1, 14)
    }
    pub(crate) fn __reduce27<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ArgsList? =  => ActionFn(32);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action32::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant16(__nt), __end));
        (0, 14)
    }
    pub(crate) fn __reduce28<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Boolean = "true" => ActionFn(24);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action24::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant17(__nt), __end));
        (1, 15)
    }
    pub(crate) fn __reduce29<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Boolean = "false" => ActionFn(25);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action25::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant17(__nt), __end));
        (1, 15)
    }
    pub(crate) fn __reduce30<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // CommaList<Arg> = Arg => ActionFn(68);
        let __sym0 = __pop_Variant8(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action68::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (1, 16)
    }
    pub(crate) fn __reduce31<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // CommaList<Arg> =  => ActionFn(69);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action69::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (0, 16)
    }
    pub(crate) fn __reduce32<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // CommaList<Arg> = (<Arg> ",")+, Arg => ActionFn(70);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant8(__symbols);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action70::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (2, 16)
    }
    pub(crate) fn __reduce33<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // CommaList<Arg> = (<Arg> ",")+ => ActionFn(71);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action71::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant15(__nt), __end));
        (1, 16)
    }
    pub(crate) fn __reduce34<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // DestinationAndFilter = ":", Lookup, FilterFn => ActionFn(66);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant12(__symbols);
        let __sym1 = __pop_Variant21(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action66::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant10(__nt), __end));
        (3, 17)
    }
    pub(crate) fn __reduce35<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // DestinationAndFilter = ":", Lookup => ActionFn(67);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant21(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action67::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant10(__nt), __end));
        (2, 17)
    }
    pub(crate) fn __reduce36<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // ExtendedIdentifier = "extended identifier" => ActionFn(22);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action22::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 18)
    }
    pub(crate) fn __reduce37<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Field = Identifier => ActionFn(7);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action7::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant18(__nt), __end));
        (1, 19)
    }
    pub(crate) fn __reduce38<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Field = ExtendedIdentifier => ActionFn(8);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action8::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant18(__nt), __end));
        (1, 19)
    }
    pub(crate) fn __reduce39<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // FilterFn = ":", FunctionOrRef => ActionFn(3);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant12(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action3::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (2, 20)
    }
    pub(crate) fn __reduce40<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Float = "float literal" => ActionFn(20);
        let __sym0 = __pop_Variant2(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action20::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant2(__nt), __end));
        (1, 21)
    }
    pub(crate) fn __reduce41<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // FunctionOrRef = QualifiedName, ArgsList => ActionFn(72);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant15(__symbols);
        let __sym0 = __pop_Variant25(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action72::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (2, 22)
    }
    pub(crate) fn __reduce42<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // FunctionOrRef = QualifiedName => ActionFn(73);
        let __sym0 = __pop_Variant25(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action73::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant12(__nt), __end));
        (1, 22)
    }
    pub(crate) fn __reduce43<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // GrokFilter = "%{", FunctionOrRef, DestinationAndFilter, "}" => ActionFn(63);
        assert!(__symbols.len() >= 4);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant10(__symbols);
        let __sym1 = __pop_Variant12(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym3.2.clone();
        let __nt = super::__action63::<>(input, __sym0, __sym1, __sym2, __sym3);
        __symbols.push((__start, __Symbol::Variant19(__nt), __end));
        (4, 23)
    }
    pub(crate) fn __reduce44<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // GrokFilter = "%{", FunctionOrRef, "}" => ActionFn(64);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant12(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action64::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant19(__nt), __end));
        (3, 23)
    }
    pub(crate) fn __reduce45<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Identifier = "identifier" => ActionFn(23);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action23::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 24)
    }
    pub(crate) fn __reduce46<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Integer = "integer literal" => ActionFn(19);
        let __sym0 = __pop_Variant3(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action19::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 25)
    }
    pub(crate) fn __reduce47<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Literal = Integer => ActionFn(14);
        let __sym0 = __pop_Variant3(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action14::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant20(__nt), __end));
        (1, 26)
    }
    pub(crate) fn __reduce48<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Literal = Float => ActionFn(15);
        let __sym0 = __pop_Variant2(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action15::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant20(__nt), __end));
        (1, 26)
    }
    pub(crate) fn __reduce49<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Literal = String => ActionFn(16);
        let __sym0 = __pop_Variant25(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action16::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant20(__nt), __end));
        (1, 26)
    }
    pub(crate) fn __reduce50<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Literal = Boolean => ActionFn(17);
        let __sym0 = __pop_Variant17(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action17::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant20(__nt), __end));
        (1, 26)
    }
    pub(crate) fn __reduce51<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Literal = Null => ActionFn(18);
        let __sym0 = __pop_Variant23(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action18::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant20(__nt), __end));
        (1, 26)
    }
    pub(crate) fn __reduce52<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Lookup = PathSegment, Lookup => ActionFn(78);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant21(__symbols);
        let __sym0 = __pop_Variant24(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action78::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant21(__nt), __end));
        (2, 27)
    }
    pub(crate) fn __reduce53<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Lookup = PathSegment => ActionFn(79);
        let __sym0 = __pop_Variant24(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action79::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant21(__nt), __end));
        (1, 27)
    }
    pub(crate) fn __reduce54<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Lookup? = Lookup => ActionFn(35);
        let __sym0 = __pop_Variant21(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action35::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (1, 28)
    }
    pub(crate) fn __reduce55<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Lookup? =  => ActionFn(36);
        let __start = __lookahead_start.cloned().or_else(|| __symbols.last().map(|s| s.2.clone())).unwrap_or_default();
        let __end = __start.clone();
        let __nt = super::__action36::<>(input, &__start, &__end);
        __symbols.push((__start, __Symbol::Variant22(__nt), __end));
        (0, 28)
    }
    pub(crate) fn __reduce56<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // Null = "null" => ActionFn(26);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action26::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant23(__nt), __end));
        (1, 29)
    }
    pub(crate) fn __reduce57<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // PathSegment = ".", Field => ActionFn(52);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant18(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action52::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant24(__nt), __end));
        (2, 30)
    }
    pub(crate) fn __reduce58<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // PathSegment = Field => ActionFn(53);
        let __sym0 = __pop_Variant18(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action53::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant24(__nt), __end));
        (1, 30)
    }
    pub(crate) fn __reduce59<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // PathSegment = "[", String, "]" => ActionFn(6);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant25(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action6::<>(input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant24(__nt), __end));
        (3, 30)
    }
    pub(crate) fn __reduce60<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // QualifiedName = Identifier => ActionFn(56);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action56::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant25(__nt), __end));
        (1, 31)
    }
    pub(crate) fn __reduce61<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // QualifiedName = Identifier, ("." Identifier)+ => ActionFn(57);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant7(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action57::<>(input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant25(__nt), __end));
        (2, 31)
    }
    pub(crate) fn __reduce62<
        'err,
        'input,
    >(
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input>,usize)>,
        _: core::marker::PhantomData<(&'err (), &'input ())>,
    ) -> (usize, usize)
    {
        // String = "string literal" => ActionFn(21);
        let __sym0 = __pop_Variant4(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action21::<>(input, __sym0);
        __symbols.push((__start, __Symbol::Variant25(__nt), __end));
        (1, 32)
    }
}
pub use self::__parse__GrokFilter::GrokFilterParser;

#[allow(unused_variables)]
fn __action0<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, GrokPattern, usize),
) -> GrokPattern
{
    __0
}

#[allow(unused_variables)]
fn __action1<
    'err,
    'input,
>(
    input: &'input str,
    (_, _, _): (usize, Tok<'input>, usize),
    (_, match_fn, _): (usize, Function, usize),
    (_, fp, _): (usize, core::option::Option<Destination>, usize),
    (_, _, _): (usize, Tok<'input>, usize),
) -> GrokPattern
{
    GrokPattern{ match_fn, destination: fp }
}

#[allow(unused_variables)]
fn __action2<
    'err,
    'input,
>(
    input: &'input str,
    (_, _, _): (usize, Tok<'input>, usize),
    (_, path, _): (usize, LookupBuf, usize),
    (_, filter_fn, _): (usize, core::option::Option<Function>, usize),
) -> Destination
{
    Destination {path: path, filter_fn}
}

#[allow(unused_variables)]
fn __action3<
    'err,
    'input,
>(
    input: &'input str,
    (_, _, _): (usize, Tok<'input>, usize),
    (_, __0, _): (usize, Function, usize),
) -> Function
{
    __0
}

#[allow(unused_variables)]
fn __action4<
    'err,
    'input,
>(
    input: &'input str,
    (_, s, _): (usize, SegmentBuf, usize),
    (_, l, _): (usize, core::option::Option<LookupBuf>, usize),
) -> LookupBuf
{
    match l {
    None => LookupBuf::from(s),
    Some(mut l) => {
      l.push_front(s);
      l
    }
  }
}

#[allow(unused_variables)]
fn __action5<
    'err,
    'input,
>(
    input: &'input str,
    (_, _, _): (usize, core::option::Option<Tok<'input>>, usize),
    (_, __0, _): (usize, FieldBuf, usize),
) -> SegmentBuf
{
    SegmentBuf::field(__0)
}

#[allow(unused_variables)]
fn __action6<
    'err,
    'input,
>(
    input: &'input str,
    (_, _, _): (usize, Tok<'input>, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, Tok<'input>, usize),
) -> SegmentBuf
{
    SegmentBuf::field(FieldBuf::from(__0))
}

#[allow(unused_variables)]
fn __action7<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> FieldBuf
{
    FieldBuf::from(__0)
}

#[allow(unused_variables)]
fn __action8<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> FieldBuf
{
    FieldBuf::from(__0)
}

#[allow(unused_variables)]
fn __action9<
    'err,
    'input,
>(
    input: &'input str,
    (_, name, _): (usize, String, usize),
    (_, args, _): (usize, core::option::Option<Vec<FunctionArgument>>, usize),
) -> Function
{
    Function { name, args }
}

#[allow(unused_variables)]
fn __action10<
    'err,
    'input,
>(
    input: &'input str,
    (_, start, _): (usize, &'input str, usize),
    (_, end, _): (usize, alloc::vec::Vec<(Tok<'input>, &'input str)>, usize),
) -> String
{
    {
    let mut name = start.to_owned();
    if !end.is_empty() {
        name.push_str(".");
        let rest: String = end.iter().map(|(t, s)| s.to_string()).collect::<Vec<String>>().join(".");
        name.push_str(rest.as_ref());
    }
    name
}
}

#[allow(unused_variables)]
fn __action11<
    'err,
    'input,
>(
    input: &'input str,
    (_, _, _): (usize, Tok<'input>, usize),
    (_, __0, _): (usize, Vec<FunctionArgument>, usize),
    (_, _, _): (usize, Tok<'input>, usize),
) -> Vec<FunctionArgument>
{
    __0
}

#[allow(unused_variables)]
fn __action12<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Value, usize),
) -> FunctionArgument
{
    FunctionArgument::Arg(__0)
}

#[allow(unused_variables)]
fn __action13<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Function, usize),
) -> FunctionArgument
{
    FunctionArgument::Function(__0)
}

#[allow(unused_variables)]
fn __action14<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, i64, usize),
) -> Value
{
    Value::Integer(__0)
}

#[allow(unused_variables)]
fn __action15<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, f64, usize),
) -> Value
{
    Value::Float(__0)
}

#[allow(unused_variables)]
fn __action16<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, String, usize),
) -> Value
{
    Value::Bytes(__0.to_string().into())
}

#[allow(unused_variables)]
fn __action17<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, bool, usize),
) -> Value
{
    Value::Boolean(__0)
}

#[allow(unused_variables)]
fn __action18<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, (), usize),
) -> Value
{
    Value::Null
}

#[allow(unused_variables)]
fn __action19<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, i64, usize),
) -> i64
{
    __0
}

#[allow(unused_variables)]
fn __action20<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, f64, usize),
) -> f64
{
    __0
}

#[allow(unused_variables)]
fn __action21<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, StringLiteral<&'input str>, usize),
) -> String
{
    __0.unescape()
}

#[allow(unused_variables)]
fn __action22<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> &'input str
{
    __0
}

#[allow(unused_variables)]
fn __action23<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> &'input str
{
    __0
}

#[allow(unused_variables)]
fn __action24<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Tok<'input>, usize),
) -> bool
{
    true
}

#[allow(unused_variables)]
fn __action25<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Tok<'input>, usize),
) -> bool
{
    false
}

#[allow(unused_variables)]
fn __action26<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Tok<'input>, usize),
) -> ()
{
    ()
}

#[allow(unused_variables)]
fn __action27<
    'err,
    'input,
>(
    input: &'input str,
    (_, mut v, _): (usize, alloc::vec::Vec<FunctionArgument>, usize),
    (_, e, _): (usize, core::option::Option<FunctionArgument>, usize),
) -> Vec<FunctionArgument>
{
    match e {
        None => v,
        Some(e) => {
            v.push(e);
            v
        }
    }
}

#[allow(unused_variables)]
fn __action28<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> alloc::vec::Vec<(Tok<'input>, &'input str)>
{
    alloc::vec![]
}

#[allow(unused_variables)]
fn __action29<
    'err,
    'input,
>(
    input: &'input str,
    (_, v, _): (usize, alloc::vec::Vec<(Tok<'input>, &'input str)>, usize),
) -> alloc::vec::Vec<(Tok<'input>, &'input str)>
{
    v
}

#[allow(unused_variables)]
fn __action30<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Tok<'input>, usize),
    (_, __1, _): (usize, &'input str, usize),
) -> (Tok<'input>, &'input str)
{
    (__0, __1)
}

#[allow(unused_variables)]
fn __action31<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Vec<FunctionArgument>, usize),
) -> core::option::Option<Vec<FunctionArgument>>
{
    Some(__0)
}

#[allow(unused_variables)]
fn __action32<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> core::option::Option<Vec<FunctionArgument>>
{
    None
}

#[allow(unused_variables)]
fn __action33<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Tok<'input>, usize),
) -> core::option::Option<Tok<'input>>
{
    Some(__0)
}

#[allow(unused_variables)]
fn __action34<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> core::option::Option<Tok<'input>>
{
    None
}

#[allow(unused_variables)]
fn __action35<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, LookupBuf, usize),
) -> core::option::Option<LookupBuf>
{
    Some(__0)
}

#[allow(unused_variables)]
fn __action36<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> core::option::Option<LookupBuf>
{
    None
}

#[allow(unused_variables)]
fn __action37<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Function, usize),
) -> core::option::Option<Function>
{
    Some(__0)
}

#[allow(unused_variables)]
fn __action38<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> core::option::Option<Function>
{
    None
}

#[allow(unused_variables)]
fn __action39<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Function, usize),
) -> Function
{
    __0
}

#[allow(unused_variables)]
fn __action40<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Destination, usize),
) -> core::option::Option<Destination>
{
    Some(__0)
}

#[allow(unused_variables)]
fn __action41<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> core::option::Option<Destination>
{
    None
}

#[allow(unused_variables)]
fn __action42<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, Destination, usize),
) -> Destination
{
    __0
}

#[allow(unused_variables)]
fn __action43<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, (Tok<'input>, &'input str), usize),
) -> alloc::vec::Vec<(Tok<'input>, &'input str)>
{
    alloc::vec![__0]
}

#[allow(unused_variables)]
fn __action44<
    'err,
    'input,
>(
    input: &'input str,
    (_, v, _): (usize, alloc::vec::Vec<(Tok<'input>, &'input str)>, usize),
    (_, e, _): (usize, (Tok<'input>, &'input str), usize),
) -> alloc::vec::Vec<(Tok<'input>, &'input str)>
{
    { let mut v = v; v.push(e); v }
}

#[allow(unused_variables)]
fn __action45<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, FunctionArgument, usize),
) -> core::option::Option<FunctionArgument>
{
    Some(__0)
}

#[allow(unused_variables)]
fn __action46<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> core::option::Option<FunctionArgument>
{
    None
}

#[allow(unused_variables)]
fn __action47<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> alloc::vec::Vec<FunctionArgument>
{
    alloc::vec![]
}

#[allow(unused_variables)]
fn __action48<
    'err,
    'input,
>(
    input: &'input str,
    (_, v, _): (usize, alloc::vec::Vec<FunctionArgument>, usize),
) -> alloc::vec::Vec<FunctionArgument>
{
    v
}

#[allow(unused_variables)]
fn __action49<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, FunctionArgument, usize),
    (_, _, _): (usize, Tok<'input>, usize),
) -> FunctionArgument
{
    __0
}

#[allow(unused_variables)]
fn __action50<
    'err,
    'input,
>(
    input: &'input str,
    (_, __0, _): (usize, FunctionArgument, usize),
) -> alloc::vec::Vec<FunctionArgument>
{
    alloc::vec![__0]
}

#[allow(unused_variables)]
fn __action51<
    'err,
    'input,
>(
    input: &'input str,
    (_, v, _): (usize, alloc::vec::Vec<FunctionArgument>, usize),
    (_, e, _): (usize, FunctionArgument, usize),
) -> alloc::vec::Vec<FunctionArgument>
{
    { let mut v = v; v.push(e); v }
}

#[allow(unused_variables)]
fn __action52<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, FieldBuf, usize),
) -> SegmentBuf
{
    let __start0 = __0.0.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action33(
        input,
        __0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action5(
        input,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
fn __action53<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, FieldBuf, usize),
) -> SegmentBuf
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action34(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action5(
        input,
        __temp0,
        __0,
    )
}

#[allow(unused_variables)]
fn __action54<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, &'input str, usize),
) -> alloc::vec::Vec<(Tok<'input>, &'input str)>
{
    let __start0 = __0.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action30(
        input,
        __0,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action43(
        input,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action55<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, alloc::vec::Vec<(Tok<'input>, &'input str)>, usize),
    __1: (usize, Tok<'input>, usize),
    __2: (usize, &'input str, usize),
) -> alloc::vec::Vec<(Tok<'input>, &'input str)>
{
    let __start0 = __1.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action30(
        input,
        __1,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action44(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action56<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, &'input str, usize),
) -> String
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action28(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action10(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action57<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, &'input str, usize),
    __1: (usize, alloc::vec::Vec<(Tok<'input>, &'input str)>, usize),
) -> String
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action29(
        input,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action10(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action58<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, FunctionArgument, usize),
    __1: (usize, Tok<'input>, usize),
) -> alloc::vec::Vec<FunctionArgument>
{
    let __start0 = __0.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action49(
        input,
        __0,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action50(
        input,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action59<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, alloc::vec::Vec<FunctionArgument>, usize),
    __1: (usize, FunctionArgument, usize),
    __2: (usize, Tok<'input>, usize),
) -> alloc::vec::Vec<FunctionArgument>
{
    let __start0 = __1.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action49(
        input,
        __1,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action51(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action60<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, core::option::Option<FunctionArgument>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.0.clone();
    let __temp0 = __action47(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action27(
        input,
        __temp0,
        __0,
    )
}

#[allow(unused_variables)]
fn __action61<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, alloc::vec::Vec<FunctionArgument>, usize),
    __1: (usize, core::option::Option<FunctionArgument>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action48(
        input,
        __0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action27(
        input,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
fn __action62<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Destination, usize),
) -> core::option::Option<Destination>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action42(
        input,
        __0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action40(
        input,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action63<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, Function, usize),
    __2: (usize, Destination, usize),
    __3: (usize, Tok<'input>, usize),
) -> GrokPattern
{
    let __start0 = __2.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action62(
        input,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action1(
        input,
        __0,
        __1,
        __temp0,
        __3,
    )
}

#[allow(unused_variables)]
fn __action64<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, Function, usize),
    __2: (usize, Tok<'input>, usize),
) -> GrokPattern
{
    let __start0 = __1.2.clone();
    let __end0 = __2.0.clone();
    let __temp0 = __action41(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action1(
        input,
        __0,
        __1,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
fn __action65<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Function, usize),
) -> core::option::Option<Function>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action39(
        input,
        __0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action37(
        input,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action66<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, LookupBuf, usize),
    __2: (usize, Function, usize),
) -> Destination
{
    let __start0 = __2.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action65(
        input,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action2(
        input,
        __0,
        __1,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action67<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, LookupBuf, usize),
) -> Destination
{
    let __start0 = __1.2.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action38(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action2(
        input,
        __0,
        __1,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action68<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, FunctionArgument, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __0.0.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action45(
        input,
        __0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action60(
        input,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action69<
    'err,
    'input,
>(
    input: &'input str,
    __lookbehind: &usize,
    __lookahead: &usize,
) -> Vec<FunctionArgument>
{
    let __start0 = __lookbehind.clone();
    let __end0 = __lookahead.clone();
    let __temp0 = __action46(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action60(
        input,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action70<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, alloc::vec::Vec<FunctionArgument>, usize),
    __1: (usize, FunctionArgument, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action45(
        input,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action61(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action71<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, alloc::vec::Vec<FunctionArgument>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action46(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action61(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action72<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, String, usize),
    __1: (usize, Vec<FunctionArgument>, usize),
) -> Function
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action31(
        input,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action9(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action73<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, String, usize),
) -> Function
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action32(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action9(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action74<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, FunctionArgument, usize),
    __2: (usize, Tok<'input>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action68(
        input,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action11(
        input,
        __0,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
fn __action75<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, Tok<'input>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __0.2.clone();
    let __end0 = __1.0.clone();
    let __temp0 = __action69(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action11(
        input,
        __0,
        __temp0,
        __1,
    )
}

#[allow(unused_variables)]
fn __action76<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, alloc::vec::Vec<FunctionArgument>, usize),
    __2: (usize, FunctionArgument, usize),
    __3: (usize, Tok<'input>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __1.0.clone();
    let __end0 = __2.2.clone();
    let __temp0 = __action70(
        input,
        __1,
        __2,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action11(
        input,
        __0,
        __temp0,
        __3,
    )
}

#[allow(unused_variables)]
fn __action77<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, Tok<'input>, usize),
    __1: (usize, alloc::vec::Vec<FunctionArgument>, usize),
    __2: (usize, Tok<'input>, usize),
) -> Vec<FunctionArgument>
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action71(
        input,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action11(
        input,
        __0,
        __temp0,
        __2,
    )
}

#[allow(unused_variables)]
fn __action78<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, SegmentBuf, usize),
    __1: (usize, LookupBuf, usize),
) -> LookupBuf
{
    let __start0 = __1.0.clone();
    let __end0 = __1.2.clone();
    let __temp0 = __action35(
        input,
        __1,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action4(
        input,
        __0,
        __temp0,
    )
}

#[allow(unused_variables)]
fn __action79<
    'err,
    'input,
>(
    input: &'input str,
    __0: (usize, SegmentBuf, usize),
) -> LookupBuf
{
    let __start0 = __0.2.clone();
    let __end0 = __0.2.clone();
    let __temp0 = __action36(
        input,
        &__start0,
        &__end0,
    );
    let __temp0 = (__start0, __temp0, __end0);
    __action4(
        input,
        __0,
        __temp0,
    )
}

pub trait __ToTriple<'err, 'input, > {
    fn to_triple(value: Self) -> Result<(usize,Tok<'input>,usize), __lalrpop_util::ParseError<usize, Tok<'input>, Error>>;
}

impl<'err, 'input, > __ToTriple<'err, 'input, > for (usize, Tok<'input>, usize) {
    fn to_triple(value: Self) -> Result<(usize,Tok<'input>,usize), __lalrpop_util::ParseError<usize, Tok<'input>, Error>> {
        Ok(value)
    }
}
impl<'err, 'input, > __ToTriple<'err, 'input, > for Result<(usize, Tok<'input>, usize), Error> {
    fn to_triple(value: Self) -> Result<(usize,Tok<'input>,usize), __lalrpop_util::ParseError<usize, Tok<'input>, Error>> {
        match value {
            Ok(v) => Ok(v),
            Err(error) => Err(__lalrpop_util::ParseError::User { error }),
        }
    }
}
