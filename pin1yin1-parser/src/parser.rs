use crate::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParserCache<'s> {
    /// really cache
    pub(crate) chars: &'s [char],
    /// the idx of the fist character in the cache
    pub(crate) first_index: usize,
    /// the idx of the next character in the cache
    ///
    /// [`Self::chars_cache_idx`] + [`Self::chars_cache.len()`]
    pub(crate) final_index: usize,
}

/// An implementation of the language parser **without** any [`Clone::clone`] call!
///
/// This implementation uses many references to increase performance(maybe...?)

#[derive(Debug, Clone, Copy)]
pub struct Parser<'s> {
    /// source codes
    pub(crate) src: &'s [char],
    /// parse state: the index of the first character in this [`ParseUnit`]
    start_idx: Option<usize>,
    /// parse state: the index of the current character in this [`ParseUnit`]
    pub(crate) idx: usize,
    /// cahce for [`ParseUnit`], increse the parse speed for [[char]]
    pub(crate) cache: ParserCache<'s>,
}

impl Parser<'_> {
    /// get the next character
    #[allow(clippy::should_implement_trait)]
    pub(crate) fn next(&mut self) -> Option<char> {
        let next = self.src.get(self.idx).copied()?;
        self.idx += 1;
        Some(next)
    }

    /// peek the next character
    pub(crate) fn peek(&self) -> Option<char> {
        self.src.get(self.idx).copied()
    }

    /// Returns the [`Parser::start_idx`] of this [`Parser`].
    ///
    /// # Panics
    ///
    /// this method should never panic
    pub(crate) fn start_idx(&self) -> usize {
        self.start_idx.unwrap()
    }
}

impl<'s> Parser<'s> {
    /// create a new parser from a slice of [char]
    pub fn new(src: &[char]) -> Parser<'_> {
        Parser {
            src,
            idx: 0,
            start_idx: None,

            cache: ParserCache {
                chars: &src[..0],
                first_index: usize::MAX,
                final_index: usize::MAX,
            },
        }
    }

    /// start a [`Try`], allow you to try many times until you get a actually [`Error`]
    /// or successfully parse a [`ParseUnit`]
    pub fn r#try<'p, F, P>(&'p mut self, p: F) -> Try<'p, 's, P>
    where
        P: ParseUnit,
        F: FnOnce(&mut Parser<'s>) -> ParseResult<'s, P>,
    {
        Try::new(self).or_try(p)
    }

    /// the internal implementation of [`Try::or_try`]
    ///
    /// a little bit tinier than [`Try::or_try`] because this will only try once
    pub fn try_once<P, F>(&mut self, parser: F) -> ParseResult<'s, P>
    where
        P: ParseUnit,
        F: FnOnce(&mut Parser<'s>) -> ParseResult<'s, P>,
    {
        // create a temp parser and reset its state
        let mut tmp = *self;
        tmp.start_idx = None;

        // do parsing
        let result = parser(&mut tmp);

        match &result {
            // if success,
            Ok(..) => {
                // foward tmp parser's work to main parser
                self.idx = tmp.idx;
                self.start_idx = self.start_idx.or(tmp.start_idx);
            }
            Err(opte) => {
                // fault
                if opte.is_some() {
                    // foward tmp parser's work to main parser
                    self.idx = tmp.idx;
                } else {
                    // synchron try cache (for &[char]::parse)
                    self.cache = tmp.cache;
                }
            }
        }

        result
    }

    /// make effort if success or return [`Error`], make no effort if failure
    pub fn parse<P: ParseUnit>(&mut self) -> ParseResult<'s, P> {
        self.try_once(P::parse)
    }

    /// set [`Self::start_idx`] to set [`Self::idx`] if [`Self::start_idx`] is unset
    ///
    /// like this method, if i dont set some of methods private in crate, someting strange
    /// behaviour will happen because of increment calling
    ///
    /// The existing [`ParseUnit`] implementation is sufficient
    pub(crate) fn start_taking(&mut self) {
        self.start_idx = Some(self.start_idx.unwrap_or(self.idx));
    }

    /// skip characters that that follow the given rule
    pub(crate) fn skip_while<Rule>(&mut self, rule: Rule) -> &mut Self
    where
        Rule: Fn(char) -> bool,
    {
        while self.peek().is_some_and(&rule) {
            self.next();
        }
        self
    }

    /// skip whitespaces
    pub(crate) fn skip_whitespace(&mut self) -> &mut Self {
        self.skip_while(|c| c.is_ascii_whitespace());
        self
    }

    /// taking characters that follow the given rule
    pub(crate) fn take_while<Rule>(&mut self, rule: Rule) -> &'s [char]
    where
        Rule: Fn(char) -> bool,
    {
        self.start_taking();
        self.skip_while(&rule);
        &self.src[self.start_idx.unwrap()..self.idx]
    }

    /// return a [`Selection`]: the selected code in this [`ParseUnit`]
    pub(crate) fn selection(&self) -> Selection<'s> {
        if self.start_idx.is_some() {
            Selection::new(self.src, self.start_idx(), self.idx - self.start_idx())
        } else {
            // while finishing parsing or throwing an error, the taking may not ever be started
            // so, match the case to make error reporting easier&better
            Selection::new(self.src, self.idx, 1)
        }
    }

    /// make a new [`Token`] with the given value and parser's selection
    pub fn new_token<I: Into<P::Target<'s>>, P: ParseUnit>(&self, t: I) -> Token<'s, P> {
        Token::new(self.selection(), t.into())
    }

    /// finish the successful parsing, just using the this method to make return easier
    pub fn finish<I: Into<P::Target<'s>>, P: ParseUnit>(&mut self, t: I) -> ParseResult<'s, P> {
        Ok(self.new_token(t))
    }

    /// make a new [`Error`] with the given value and parser's selection
    pub fn new_error(&mut self, reason: impl Into<String>) -> Error<'s> {
        Error::new(self.selection(), reason.into())
    }

    /// finish the faild parsing, just using the this method to make return easier
    ///
    /// **you should return this method's return value to throw an error!!!**
    pub fn throw<P: ParseUnit>(&mut self, reason: impl Into<String>) -> ParseResult<'s, P> {
        Err(Some(self.new_error(reason)))
    }
}

/// a [`Try`], allow you to try many times until you get a actually [`Error`]
/// or successfully parse a [`ParseUnit`]
pub struct Try<'p, 's, P: ParseUnit> {
    parser: &'p mut Parser<'s>,
    state: Option<std::result::Result<Token<'s, P>, Error<'s>>>,
    /// TODO, i wonder parallel is even slower
    #[cfg(feature = "parallel")]
    tasks: tokio::task::JoinSet<ParseResult<'s, P>>,
}

impl<'p, 's, P: ParseUnit> Try<'p, 's, P> {
    pub fn new(parser: &'p mut Parser<'s>) -> Self {
        Self {
            parser,
            state: None,
        }
    }

    /// try once again
    ///
    /// do noting if the [`Try`] successfully parsed the [`ParseUnit`],
    /// or got a actually [`Error`]
    pub fn or_try<P1, F>(mut self, parser: F) -> Self
    where
        P1: ParseUnit<Target<'s> = P::Target<'s>>,
        F: FnOnce(&mut Parser<'s>) -> ParseResult<'s, P1>,
    {
        if self.state.is_some() {
            return self;
        }

        self.state = match self.parser.try_once(parser) {
            Ok(tk) => Some(Ok(Token::new(tk.selection, tk.target))),
            Err(Some(e)) => Some(Err(e)),
            _ => self.state,
        };

        self
    }

    /// set the default error
    pub fn or_error(mut self, reason: impl Into<String>) -> Self {
        self.state = self
            .state
            .or_else(|| Some(Err(self.parser.new_error(reason))));
        self
    }

    /// finish parsing tring
    ///
    /// its not recommended to return [`Err`] with [`None`]
    ///
    /// there should be at least one [`Self::or_try`] return [`Err`] with [`Some`] in,
    /// or the parser will throw a message with very bad readability
    pub fn finish(self) -> ParseResult<'s, P> {
        match self.state {
            Some(r) => r.map_err(Some),
            None => Err(None),
        }
    }
}
