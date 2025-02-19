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
//! * `utils.path.split(path: string) -> parent, filename`
//!
//! NOTE 1: Currently, `utils.path.join` uses `utils.path.join_os_non_normalized`. This might change in the future.
//!
//! NOTE 2: The reason why normalized is prefixed with `_os_`
//!         is because there is another type of normalization that removes the "../".
//!         There are no functions for this yet, but keeping the future open.

use crate::run::{PathResolver, RuntimeContext};
use crate::Result;
use mlua::{Lua, MultiValue, Table};
use mlua::{Value, Variadic};
use simple_fs::SPath;
use std::path::PathBuf;
use std::path::{Path, MAIN_SEPARATOR};
use std::path::Component;

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
	let path = ctx.dir_context().resolve_path(&path, PathResolver::WorkspaceDir)?;
	Ok(path.exists())
}

/// ## Lua Documentation
/// ```lua
/// path.is_file(path: string) -> bool
/// ```
///
/// Checks if the specified path is a file.
fn path_is_file(ctx: &RuntimeContext, path: String) -> mlua::Result<bool> {
	let path = ctx.dir_context().resolve_path(&path, PathResolver::WorkspaceDir)?;
	Ok(path.is_file())
}

/// ## Lua Documentation
/// ```lua
/// path.is_dir(path: string) -> bool
/// ```
///
/// Checks if the specified path is a directory.
fn path_is_dir(ctx: &RuntimeContext, path: String) -> mlua::Result<bool> {
	let path = ctx.dir_context().resolve_path(&path, PathResolver::WorkspaceDir)?;
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
pub fn path_join_non_os_normalized(lua: &Lua, paths: Variadic<Value>) -> mlua::Result<Value> {
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
pub fn path_join_os_normalized(lua: &Lua, paths: Variadic<Value>) -> mlua::Result<Value> {
    // Collect normalized path components as OsStrings.
    let mut components = Vec::new();

    if paths.is_empty() {
        return Ok(mlua::Value::Nil);
    }

    // Helper closure to break a &str into its normalized components.
    let mut process_str = |s: &str| {
        if !s.is_empty() {
            // Create a Path and iterate over its normalized components.
            let path = Path::new(s);
            for comp in path.components() {
                components.push(comp.as_os_str().to_os_string());
            }
        }
    };

    // If the first argument is a table, treat it as a table of strings.
    if let Some(mlua::Value::Table(table)) = paths.first() {
        for pair in table.clone().pairs::<mlua::Integer, String>() {
            let (_, s) = pair?;
            process_str(&s);
        }
    } else {
        // Otherwise, treat each Lua value as a string.
        for arg in paths {
            if let mlua::Value::String(s) = arg {
                let s_str = s.to_str()?;
                process_str(&s_str);
            }
        }
    }

    // If no valid components were gathered, return an empty string.
    if components.is_empty() {
        return Ok(mlua::Value::String(lua.create_string("")?));
    }

    // Rebuild the path from the collected components.
    let mut joined = PathBuf::new();
    for comp in &components {
        joined.push(comp);
    }

    // Convert the joined path to a Rust String using the OS-native formatting.
    let result = joined
        .into_os_string()
        .into_string()
        .unwrap_or_else(|os_str| os_str.to_string_lossy().into_owned());

    Ok(mlua::Value::String(lua.create_string(&result)?))
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

	// NOTE 1: Here we are testing these functions in the context of an agent to ensure they work in that context.
	//         A more purist approach would be to test the Lua functions in a blank Lua engine, but the net value of testing
	//         them in the context where they will run is higher. Height wins.
	//
	// NOTE 2: All the tests here are with run_reflective_agent that have the tests-data/sandbox-01 as current dir.
	type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

	use std::path::MAIN_SEPARATOR;

	use crate::_test_support::{eval_lua, run_reflective_agent, setup_lua};
	use crate::run::Runtime;

	#[tokio::test]
	async fn test_lua_path_exists_true() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let paths = &[
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
			let code = format!(r#"return utils.path.exists("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			assert!(res.as_bool().ok_or("Result should be a bool")?, "'{path}' should exist");
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_exists_false() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let paths = &["./no file .rs", "some/no-file.md", "./s do/", "no-dir/at/all"];

		// -- Exec & Check
		for path in paths {
			let code = format!(r#"return utils.path.exists("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			assert!(
				!res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should NOT exist"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_file_true() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let paths = &[
			"./agent-script/agent-hello.devai",
			"agent-script/agent-hello.devai",
			"./sub-dir-a/agent-hello-2.devai",
			"sub-dir-a/agent-hello-2.devai",
			"./sub-dir-a/../agent-script/agent-hello.devai",
		];

		// -- Exec & Check
		for path in paths {
			let code = format!(r#"return utils.path.is_file("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			assert!(
				res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should be a file"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_file_false() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let paths = &["./no-file", "no-file.txt", "sub-dir-a/"];

		// -- Exec & Check
		for path in paths {
			let code = format!(r#"return utils.path.is_file("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			assert!(
				!res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should NOT be a file"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_dir_true() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let paths = &["./sub-dir-a", "sub-dir-a", "./sub-dir-a/.."];

		// -- Exec & Check
		for path in paths {
			let code = format!(r#"return utils.path.is_dir("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			assert!(
				res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should be a directory"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_is_dir_false() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let paths = &[
			"./agent-hello.devai",
			"agent-hello.devai",
			"./sub-dir-a/agent-hello-2.devai",
			"./sub-dir-a/other-path",
			"nofile.txt",
			"./s rc/",
		];

		// -- Exec & Check
		for path in paths {
			let code = format!(r#"return utils.path.is_dir("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			assert!(
				!res.as_bool().ok_or("Result should be a bool")?,
				"'{path}' should NOT be a directory"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_parent() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		// Fixtures: (path, expected_parent)
		let paths = &[
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
			let code = format!(r#"return utils.path.parent("{path}")"#);
			let res = eval_lua(&lua, &code)?;
			let result = res.as_str().ok_or("Should be a string")?;
			assert_eq!(result, *expected, "Parent mismatch for path: {path}");
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_split() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
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
			let code = format!(
				r#"
                    local parent, filename = utils.path.split("{path}")
                    return {{ parent, filename }} -- Wrap values in a Lua table
                "#
			);
			let res = eval_lua(&lua, &code)?;
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
	fn test_lua_path_join_os_non_normalized() -> Result<()> {
		common_test_lua_path_join_non_os_normalized("join_os_non_normalized")?;
		Ok(())
	}

	#[test]
	fn test_lua_path_join_os_normalized_lua_engine() -> Result<()> {
		common_test_lua_path_join_os_normalized_lua_engine("join_os_normalized")?;
		Ok(())
	}

	#[tokio::test]
	async fn test_lua_path_join_os_normalized_reflective() -> Result<()> {
		common_test_lua_path_join_os_normalized_reflective("join_os_normalized").await?;
		Ok(())
	}

	// region:    --- Tests Support

	fn common_test_lua_path_join_non_os_normalized(join_fn_name: &str) -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
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

		// -- Exec & Check
		for (input, expected) in cases {
			let code = format!("return utils.path.{join_fn_name}({})", input);
			let result: String = lua.load(&code).eval()?;
			assert_eq!(result, expected, "Non-normalized failed for input: {}", input);
		}

		Ok(())
	}

	async fn common_test_lua_path_join_os_normalized_reflective(join_fn_name: &str) -> Result<()> {
		// -- Setup & Fixtures
		let cases = &[
			(
				r#"{"folder", "subfolder", "file.txt"}"#,
				format!("folder{}subfolder{}file.txt", MAIN_SEPARATOR, MAIN_SEPARATOR),
			),
			(r#"{"single"}"#, "single".to_string()),
			(r#"{"leading", "", "trailing"}"#, 
                format!("leading{}trailing", MAIN_SEPARATOR)
            ),
			(
				r#"{"C:\\Users", "Admin", "Documents\\file.txt"}"#,
				"C:\\Users\\Admin\\Documents\\file.txt".to_string(),
			),
			(
				r#"{"C:/Users", "Admin", "Documents/file.txt"}"#,
				"C:\\Users\\Admin\\Documents\\file.txt".to_string(),
			),
			(r#"{"C:/", "Windows", "System32"}"#, "C:\\Windows\\System32".to_string()),
		];

		// -- Exec & Check
		for (lua_table, expected_path) in cases {
			let code = format!(r#"return utils.path.{}({lua_table})"#, join_fn_name);
			let res = run_reflective_agent(&code, None).await?;
			let result_path = res.as_str().ok_or("Should return a string")?;
			assert_eq!(result_path, expected_path, "Path mismatch for table input: {lua_table}");
		}

		Ok(())
	}

	fn common_test_lua_path_join_os_normalized_lua_engine(join_fn_name: &str) -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::init_module, "path")?;
		let sep = MAIN_SEPARATOR;
		let cases = vec![
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
			(
				r#"{"C:/Users", "Admin", "Documents/file.txt"}"#,
				"C:\\Users\\Admin\\Documents\\file.txt".to_string(),
			),
			(
				r#"{"\\server", "share", "folder", "file.txt"}"#,
				"\\server\\share\\folder\\file.txt".to_string(),
			),
		];

		// -- Exec & Check
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
