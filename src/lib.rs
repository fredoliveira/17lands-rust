// SPDX-License-Identifier: GPL-3.0-only
//
// seventeenlands-rust — a Rust port of the 17Lands MTG Arena log client.
// Copyright (C) 2026 Fred Oliveira <fred@helloform.com>
//
// This program is a derivative work of mtga-log-client
// (https://github.com/rconroy293/mtga-log-client), Copyright (C) its authors
// (rconroy293), licensed under the GNU General Public License v3.0.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License, version 3, as published by the
// Free Software Foundation. This program is distributed in the hope that it will
// be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public
// License for more details. You should have received a copy of the GNU General
// Public License along with this program. If not, see
// <https://www.gnu.org/licenses/>.

//! 17Lands MTGA log client — Rust port (library crate).
//!
//! Drop-in replacement for the Python `seventeenlands` client. The `main` binary is a thin
//! wrapper over this crate; the modules are public so the integration tests in `tests/`
//! (fixture/oracle parity, HTTP) can drive them directly.

pub mod api_client;
pub mod config;
pub mod follower;
pub mod paths;
pub mod retry;
pub mod time_parse;
