#![warn(missing_docs)]
//! Various fnmatch- and glob-style handling.
//!
//! For the present, this crate only defines a conversion function from
//! an fnmatch-style glob pattern to a regular expression.
//!
//! See the [`glob`] module for more information on
//! the [`glob_to_regex`] function's usage.

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
// Activate most of the clippy::restriction lints that we have come across...
#![warn(clippy::exhaustive_enums)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(clippy::missing_inline_in_public_items)]
#![warn(clippy::panic)]
#![warn(clippy::pattern_type_mismatch)]
#![warn(clippy::shadow_reuse)]
#![warn(clippy::shadow_same)]
#![warn(clippy::str_to_string)]
// ...except for these ones.
#![allow(clippy::implicit_return)]
// Activate most of the clippy::pedantic lints that we have come across...
#![warn(clippy::explicit_into_iter_loop)]
#![warn(clippy::match_bool)]
#![warn(clippy::missing_errors_doc)]
#![warn(clippy::panic_in_result_fn)]
#![warn(clippy::too_many_lines)]
#![warn(clippy::unnecessary_wraps)]
#![warn(clippy::unreachable)]
// ...except for these ones.
#![allow(clippy::module_name_repetitions)]
// Activate most of the clippy::nursery lints that we have come across...
#![warn(clippy::branches_sharing_code)]
#![warn(clippy::missing_const_for_fn)]

pub mod error;
pub mod glob;

pub use glob::glob_to_regex_string;
