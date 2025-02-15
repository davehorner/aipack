//! Defines the `path` module, used in the lua engine.
//!
//! ---
//!
//! ## Lua documentation
//! The `path` module exposes functions used to interact with file paths.
//!
//! ### Functions
//! * `utils.path.exists(path: string) -> bool`
//! * `utils.path.is_file(path: string) -> bool`
//! * `utils.path.is_dir(path: string) -> bool`
//! * `utils.path.parent(path: string) -> string | nil`
//! * `utils.path.join(path: string) -> string | nil` (default non os normalized)
//! * `utils.path.join_os_normalized(path: string) -> string | nil` (windows style if start with like C:)
//! * `utils.path.join_os_non_normalized(path: string) -> string | nil` (default, as user specified)
//! * `path.split(path: string) -> parent, filename`
//!
//! NOTE 1: Currently, `utils.path.join` uses `utils.path.join_os_non_normalized`. This might change in the future.
//!
//! NOTE 2: The reason why normalized is prefixed with `_os_`
//!         is because there is another type of normalization that removes the "../".
//!         There are no functions for this yet, but keeping the future open.

use crate::run::{PathResolver, RuntimeContext};
use mlua::{Lua, MultiValue, Result, Table};
use mlua::{Value, Variadic};
use simple_fs::SPath;
use std::path::PathBuf;
use std::path::{Path, MAIN_SEPARATOR};

pub fn init_module(lua: &Lua, runtime_context: &RuntimeContext) -> Result<Table> {
	let table = lua.create_table()?;

	// -- split
	let path_split_fn = lua.create_function(path_split)?;

	// -- exists
	let ctx = runtime_context.clone();
	let path_exists_fn = lua.create_function(move |_lua, path: String| path_exists(&ctx, path))?;

	// -- is_file
	let ctx = runtime_context.clone();
	let path_is_file_fn = lua.create_function(move |_lua, path: String| path_is_file(&ctx, path))?;

	// -- is_dir
	let ctx = runtime_context.clone();
	let path_is_dir_fn = lua.create_function(move |_lua, path: String| path_is_dir(&ctx, path))?;

	// -- parent
	let path_parent_fn = lua.create_function(move |_lua, path: String| path_parent(path))?;

	// -- joins
	let path_join_non_os_normalized_fn = lua.create_function(path_join_non_os_normalized)?;
	let path_join_os_normalized_fn = lua.create_function(path_join_os_normalized)?;
	let path_join_fn = lua.create_function(path_join_non_os_normalized)?;

	// -- Add all functions to the module
	table.set("exists", path_exists_fn)?;
	table.set("is_file", path_is_file_fn)?;
	table.set("is_dir", path_is_dir_fn)?;
	table.set("parent", path_parent_fn)?;
	table.set("join", path_join_fn)?;
	table.set("join_os_non_normalized", path_join_non_os_normalized_fn)?;
	table.set("join_os_normalized", path_join_os_normalized_fn)?;
	table.set("split", path_split_fn)?;

	Ok(table)
}

// region:    --- Lua Functions

/// ## Lua Documentation
/// ```lua
/// path.split(path: string) -> parent, filename
/// ```
/// {utils.path.split("some/path/to_file.md")} to create an array.
///
/// Split path into parent, filename.
fn path_split(lua: &Lua, path: String) -> mlua::Result<MultiValue> {
	let path = SPath::from(path);

	let parent = path.parent().map(|p| p.to_string()).unwrap_or_default();
	let file_name = path.file_name().unwrap_or_default().to_string();

	Ok(MultiValue::from_vec(vec![
		mlua::Value::String(lua.create_string(parent)?),
		mlua::Value::String(lua.create_string(file_name)?),
	]))
}

/// ## Lua Documentation
/// ```lua
/// path.exists(path: string) -> bool
/// ```
///
/// Checks if the specified path exists.
fn path_exists(ctx: &RuntimeContext, path: String) -> mlua::Result<bool> {
	let path = ctx.dir_context().resolve_path(&path, PathResolver::DevaiParentDir)?;
	Ok(path.exists())
}

/// ## Lua Documentation
/// ```lua
/// path.is_file(path: string) -> bool
/// ```
///
/// Checks if the specified path is a file.
fn path_is_file(ctx: &RuntimeContext, path: String) -> mlua::Result<bool> {
	let path = ctx.dir_context().resolve_path(&path, PathResolver::DevaiParentDir)?;
	Ok(path.is_file())
}

/// ## Lua Documentation
/// ```lua
/// path.is_dir(path: string) -> bool
/// ```
///
/// Checks if the specified path is a directory.
fn path_is_dir(ctx: &RuntimeContext, path: String) -> mlua::Result<bool> {
	let path = ctx.dir_context().resolve_path(&path, PathResolver::DevaiParentDir)?;
	Ok(path.is_dir())
}

/// ## Lua Documentation
/// ```lua
/// path.parent(path: string) -> string | nil
/// ```
///
/// Returns the parent directory of the specified path, or nil if there is no parent.
/// (follows the Rust Path::parent(&self) logic)
fn path_parent(path: String) -> mlua::Result<Option<String>> {
	match Path::new(&path).parent() {
		Some(parent) => match parent.to_str() {
			Some(parent_str) => Ok(Some(parent_str.to_string())),
			None => Ok(None),
		},
		None => Ok(None),
	}
}

/// ## Lua Documentation
/// ```lua
/// path.join(path: string) -> string | nil
///
/// -- Table example:
/// local paths = {"folder", "subfolder", "file.txt"}
/// local full_path = utils.path.join(paths)
///
/// -- Arg example:
/// local full_path = utils.path.join("folder", "subfolder", "file.txt")
///
/// ```
///
/// Returns the path, with paths joined.
/// (follows the Rust PathBuf::join(&self) logic)
///
/// Joins path components without OS normalization.
///
/// JC NOTE: This might be deprecated at some oint
///
/// This version uses Rust’s `PathBuf` join logic without altering the components (apart from ignoring empty strings).
/// Any mixed or extra separators are preserved.
pub fn path_join_non_os_normalized(lua: &Lua, paths: Variadic<Value>) -> Result<Value> {
	let mut path_buf = PathBuf::new();
	if paths.is_empty() {
		return Ok(Value::Nil);
	}
	// If the first argument is a table, iterate over its entries.
	if let Some(Value::Table(table)) = paths.first() {
		for pair in table.clone().pairs::<mlua::Integer, String>() {
			let (_, s) = pair?;
			path_buf.push(s);
		}
	} else {
		// Otherwise, iterate over the variadic arguments.
		for arg in paths {
			if let Value::String(s) = arg {
				path_buf.push(s.to_str()?.to_string());
			}
		}
	}
	Ok(Value::String(lua.create_string(path_buf.to_string_lossy().as_ref())?))
}

/// Joins path components with OS normalization.
///
/// This version first gathers nonempty strings then “normalizes” each component by trimming extra leading
/// and trailing slashes. If the first component looks like a Windows path (i.e. its second character is a colon,
/// e.g. `"C:"`, or it starts with a backslash), then the join is done using backslashes (and any forward slashes
/// in the components are converted to backslashes). Otherwise, the platform’s native separator is used.
pub fn path_join_os_normalized(lua: &Lua, paths: Variadic<Value>) -> Result<Value> {
	let mut comps = Vec::new();
	if paths.is_empty() {
		return Ok(Value::Nil);
	}
	if let Some(Value::Table(table)) = paths.first() {
		for pair in table.clone().pairs::<mlua::Integer, String>() {
			let (_, s) = pair?;
			if !s.is_empty() {
				comps.push(s);
			}
		}
	} else {
		for arg in paths {
			if let Value::String(s) = arg {
				let s = s.to_str()?;
				if !s.is_empty() {
					comps.push(s.to_string());
				}
			}
		}
	}
	if comps.is_empty() {
		return Ok(Value::String(lua.create_string("")?));
	}
	let is_windows = is_windows_style(&comps[0]);
	let sep: char = if is_windows { '\\' } else { MAIN_SEPARATOR };
	let mut result = String::new();
	if is_windows {
		// For Windows‑style, trim trailing slashes from the first component and convert any '/' to '\\'.
		let first = comps[0].trim_end_matches(['\\', '/']).replace("/", "\\");
		result.push_str(&first);
		for comp in comps.iter().skip(1) {
			// For subsequent components, trim both leading and trailing slashes and convert '/' to '\\'.
			let part = comp.trim_matches(|c| c == '\\' || c == '/').replace("/", "\\");
			if !part.is_empty() {
				if !result.ends_with(sep) {
					result.push(sep);
				}
				result.push_str(&part);
			}
		}
	} else {
		// For non–Windows style, simply trim extra slashes.
		let first = comps[0].trim_end_matches(['\\', '/']);
		result.push_str(first);
		for comp in comps.iter().skip(1) {
			let part = comp.trim_matches(|c| c == '\\' || c == '/');
			if !part.is_empty() {
				if !result.ends_with(sep) {
					result.push(sep);
				}
				result.push_str(part);
			}
		}
	}
	Ok(Value::String(lua.create_string(&result)?))
}

/// Returns true if the given string looks like a Windows‑style path.
/// That is, if its second character is a colon (e.g. `"C:"`) or it starts with a backslash.
fn is_windows_style(s: &str) -> bool {
	(s.len() >= 2 && s.as_bytes()[1] == b':') || s.starts_with('\\')
}

// endregion: --- Lua Functions

// region:    --- Tests

#[cfg(test)]
mod tests {
	use super::*;
	use mlua::Lua;

	/// Sets up a Lua instance with both functions registered under `utils.path`.
	fn setup_lua() -> Result<Lua> {
		let runtime = Runtime::new_test_runtime_sandbox_01()?;

		let lua = Lua::new();
		let globals = lua.globals();
		let utils = lua.create_table().unwrap();
		// let path_table = lua.create_table().unwrap();

		let path_table = init_module(&lua, &runtime.context())?;
		utils.set("path", path_table).unwrap();
		globals.set("utils", utils).unwrap();
		Ok(lua)
	}

	// NOTE 1: Here we are testing these functions in the context of an agent to ensure they work in that context.
	//         A more purist approach would be to test the Lua functions in a blank Lua engine, but the net value of testing
	//         them in the context where they will run is higher. Height wins.
	//
	// NOTE 2: All the tests here are with run_reflective_agent that have the tests-data/sandbox-01 as current dir.
	type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

	use std::path::MAIN_SEPARATOR;

	use crate::_test_support::run_reflective_agent;
	use crate::run::Runtime;

	#[tokio::test]
	async fn test_lua_path_exists_true() -> Result<()> {
		// -- Fixtures
		let paths = &[
			//
			"./agent-script/agent-hello.devai",
			"agent-script/agent-hello.devai",
			"./sub-dir-a/agent-hello-2.devai",
			"sub-dir-a/agent-hello-2.devai",
			"sub-dir-a/",
			"sub-dir-a",
			"./sub-dir-a/",
			"./sub-dir-a/../",
			"./sub-dir-a/..",
		];

		// -- Exec & Check
		for path in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.exists("{path}")"#), None).await?;
			assert!(
				res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should exists"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_exists_false() -> Result<()> {
		// -- Fixtures
		let paths = &[
			//
			"./no file .rs",
			"some/no-file.md",
			"./s do/",
			"no-dir/at/all",
		];

		// -- Exec & Check
		for path in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.exists("{path}")"#), None).await?;
			assert!(
				!res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should NOT exists"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_file_true() -> Result<()> {
		// -- Fixtures
		let paths = &[
			//
			"./agent-script/agent-hello.devai",
			"agent-script/agent-hello.devai",
			"./sub-dir-a/agent-hello-2.devai",
			"sub-dir-a/agent-hello-2.devai",
			"sub-dir-a/../agent-script/agent-hello.devai",
		];

		// -- Exec & Check
		for path in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.is_file("{path}")"#), None).await?;
			assert!(
				res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should be is_file"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_file_false() -> Result<()> {
		// -- Fixtures
		let paths = &[
			//
			"./no-file",
			"no-file.txt",
			"sub-dir-a/",
		];

		// -- Exec & Check
		for path in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.is_file("{path}")"#), None).await?;
			assert!(
				!res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should NOT be is_file"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_dir_true() -> Result<()> {
		// -- Fixtures
		let paths = &[
			//
			"./sub-dir-a",
			"sub-dir-a",
			"./sub-dir-a/..",
			// Note: below does not work for now becsuse some-other-dir does not exists. Might want to use clean.
			// "./sub-dir-a/some-other-dir/..",
		];

		// -- Exec & Check
		for path in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.is_dir("{path}")"#), None).await?;
			assert!(
				res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should be is_dir"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_dir_false() -> Result<()> {
		// -- Fixtures
		let paths = &[
			//
			"./agent-hello.devai",
			"agent-hello.devai",
			"./sub-dir-a/agent-hello-2.devai",
			"./sub-dir-a/other-path",
			"nofile.txt",
			"./s rc/",
		];

		// -- Exec & Check
		for path in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.is_dir("{path}")"#), None).await?;
			assert!(
				!res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should NOT be is_dir"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_parent() -> Result<()> {
		// -- Fixtures
		// This is the rust Path logic
		let paths = &[
			//
			("./agent-hello.devai", "."),
			("./", ""),
			(".", ""),
			("./sub-dir/file.txt", "./sub-dir"),
			("./sub-dir/file", "./sub-dir"),
			("./sub-dir/", "."),
			("./sub-dir", "."),
		];

		// -- Exec & Check
		for (path, expected) in paths {
			let res = run_reflective_agent(&format!(r#"return utils.path.parent("{path}")"#), None).await?;
			let res = res.as_str().ok_or("Should be a string")?;
			assert_eq!(res, *expected);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_split() -> Result<()> {
		// -- Fixtures
		let paths = &[
			("some/path/to_file.md", "some/path", "to_file.md"),
			("folder/file.txt", "folder", "file.txt"),
			("justafile.md", "", "justafile.md"), // No parent directory
			("/absolute/path/file.log", "/absolute/path", "file.log"),
			("/file_at_root", "/", "file_at_root"),
			("trailing/slash/", "trailing", "slash"), // Directory with no file
		];

		// -- Exec & Check
		for (path, expected_parent, expected_filename) in paths {
			let res = run_reflective_agent(
				&format!(
					r#"
                        local parent, filename = utils.path.split("{path}")
                        return {{ parent, filename }} -- Wrap values in a Lua table
                    "#
				),
				None,
			)
			.await?;

			let res_array = res.as_array().ok_or("Expected an array from Lua function")?;

			let parent = res_array
				.first()
				.and_then(|v| v.as_str())
				.ok_or("First value should be a string")?;

			let filename = res_array
				.get(1)
				.and_then(|v| v.as_str())
				.ok_or("Second value should be a string")?;

			assert_eq!(parent, *expected_parent, "Parent mismatch for path: {path}");
			assert_eq!(filename, *expected_filename, "Filename mismatch for path: {path}");
		}

		Ok(())
	}

	#[test]
	fn test_lua_path_join_default() -> Result<()> {
		common_test_lua_path_join_non_os_normalized("join")?;
		Ok(())
	}

	#[test]
	fn test_path_join_os_non_normalized() -> Result<()> {
		common_test_lua_path_join_non_os_normalized("join_os_non_normalized")?;
		Ok(())
	}

	#[test]
	fn test_path_join_os_normalized_lua_engine() -> Result<()> {
		common_test_lua_path_join_os_normalized_lua_engine("join_os_normalized")?;
		Ok(())
	}

	#[tokio::test]
	async fn test_path_join_os_normalized_reflective() -> Result<()> {
		common_test_lua_path_join_os_normalized_reflective("join_os_normalized").await?;
		Ok(())
	}

	// region:    --- Tests Support

	fn common_test_lua_path_join_non_os_normalized(join_fn_name: &str) -> Result<()> {
		let lua = setup_lua()?;
		let mut expected1 = PathBuf::new();
		expected1.push("folder");
		expected1.push("subfolder");
		expected1.push("file.txt");

		let mut expected2 = PathBuf::new();
		expected2.push("folder\\");
		expected2.push("subfolder/");
		expected2.push("file.txt");

		let cases = vec![
			(
				r#"{"folder", "subfolder", "file.txt"}"#,
				expected1.to_string_lossy().to_string(),
			),
			(
				r#"{"folder\\", "subfolder/", "file.txt"}"#,
				expected2.to_string_lossy().to_string(),
			),
		];
		for (input, expected) in cases {
			let code = format!("return utils.path.{join_fn_name}({})", input);
			let result: String = lua.load(&code).eval()?;
			assert_eq!(result, expected, "Non-normalized failed for input: {}", input);
		}
		Ok(())
	}

	async fn common_test_lua_path_join_os_normalized_reflective(join_fn_name: &str) -> Result<()> {
		// -- Fixtures
		let cases = &[
			// Standard paths
			(
				r#"{"folder", "subfolder", "file.txt"}"#,
				format!("folder{}subfolder{}file.txt", MAIN_SEPARATOR, MAIN_SEPARATOR),
			),
			(r#"{"single"}"#, "single".to_string()),
			(r#"{"leading", "", "trailing"}"#, "leading/trailing".to_string()), // Handles empty segments
			(
				r#"{"C:\\Users", "Admin", "Documents\\file.txt"}"#,
				"C:\\Users\\Admin\\Documents\\file.txt".to_string(),
			),
			(
				r#"{"C:/Users", "Admin", "Documents/file.txt"}"#,
				"C:\\Users\\Admin\\Documents\\file.txt".to_string(),
			),
			(r#"{"C:/", "Windows", "System32"}"#, "C:\\Windows\\System32".to_string()),
			//
			// IMPORTANT: The tests below are out of scope for now for the os_normalization
			//
			// Mixed path separators
			// DISABLE FOR NOW does not work on mac apparently.
			// (
			// 	r#"{"folder\\", "subfolder/", "file.txt"}"#,
			// 	format!("folder{}subfolder{}file.txt", MAIN_SEPARATOR, MAIN_SEPARATOR),
			// ),
			//
			// Absolute paths/ UNC Type
			// DISABLE - This won't path on unixy, because of the current logic
			//           But UNC type of path is out of scope for now
			// (
			// 	r#"{"\\server", "share", "folder", "file.txt"}"#,
			// 	format!(
			// 		"{}server{}share{}folder{}file.txt",
			// 		MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR
			// 	),
			// ),
		];

		for (lua_table, expected_path) in cases {
			let code = format!(r#"return utils.path.{}({lua_table})"#, join_fn_name);
			let res = run_reflective_agent(&code, None).await?;

			let result_path = res.as_str().ok_or("Should return a string")?;
			assert_eq!(result_path, expected_path, "Path mismatch for table input: {lua_table}");
		}

		Ok(())
	}

	fn common_test_lua_path_join_os_normalized_lua_engine(join_fn_name: &str) -> Result<()> {
		let lua = setup_lua()?;
		let sep = MAIN_SEPARATOR;
		let cases = vec![
			// Standard (non-Windows) paths.
			(
				r#"{"folder", "subfolder", "file.txt"}"#,
				format!("folder{sep}subfolder{sep}file.txt", sep = sep),
			),
			(
				r#"{"leading", "", "trailing"}"#,
				format!("leading{sep}trailing", sep = sep),
			),
			(
				r#"{"folder\\", "subfolder/", "file.txt"}"#,
				format!("folder{sep}subfolder{sep}file.txt", sep = sep),
			),
			// Windows‑style paths.
			(
				r#"{"C:/Users", "Admin", "Documents/file.txt"}"#,
				"C:\\Users\\Admin\\Documents\\file.txt".to_string(),
			),
			(
				r#"{"\\server", "share", "folder", "file.txt"}"#,
				"\\server\\share\\folder\\file.txt".to_string(),
			),
		];
		for (input, expected) in cases {
			let code = format!("return utils.path.{join_fn_name}({})", input);
			let result: String = lua.load(&code).eval()?;
			assert_eq!(result, expected, "Normalized failed for input: {}", input);
		}
		Ok(())
	}
	// endregion: --- Tests Support
}

// endregion: --- Tests
