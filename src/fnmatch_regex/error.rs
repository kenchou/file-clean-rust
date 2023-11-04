//! Common error definitions for the fnmatch crate.

/*
 * Copyright (c) 2021, 2022  Peter Pentchev <roam@ringlet.net>
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */

use quick_error::quick_error;

quick_error! {
    /// An error that occurred during the processing of a pattern.
    #[derive(Debug)]
    #[non_exhaustive]
    pub enum Error {
        /// A bare escape character at the end of the pattern.
        BareEscape {
            display("Bare escape character")
        }
        /// The resulting regex was invalid.
        InvalidRegex(pattern: String, error: String) {
            display("Could not compile the resulting pattern {:?}: {}", pattern, error)
        }
        /// Some known missing functionality.
        NotImplemented(message: String) {
            display("Not implemented yet: {}", message)
        }
        /// An invalid combination of ranges ([a-b-c]) within a character class.
        RangeAfterRange(start: char, end: char) {
            display("Range following a {:?}-{:?} range", start, end)
        }
        /// A reversed range within a character class.
        ReversedRange(start: char, end: char) {
            display("Reversed range from {:?} to {:?}", start, end)
        }
        /// An alternation that was not closed before the end of the pattern.
        UnclosedAlternation {
            display("Unclosed alternation")
        }
        /// A character class that was not closed before the end of the pattern.
        UnclosedClass {
            display("Unclosed character class")
        }
    }
}
