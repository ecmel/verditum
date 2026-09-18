/*
 * Copyright (c) 1986-2026 Ecmel Ercan
 * See LICENSE in the project root for license information.
 */

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  verditum_lib::run()
}
