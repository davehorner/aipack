use crate::dir_context::{DirContext, PathResolver};
use crate::hub::get_hub;
use crate::run::RuntimeContext;
use crate::script::LuaValueExt;
use crate::script::lua_script::helpers::{get_value_prop_as_string, to_vec_of_strings};
use crate::support::{AsStrsExt, files, paths};
use crate::types::{FileMeta, FileRecord};
use crate::{Error, Result};
use mlua::{FromLua, IntoLua, Lua, Value};
use simple_fs::{ListOptions, SPath, ensure_file_dir, iter_files, list_files};
use std::fs::write;
use std::io::Write;

/// ## Lua Documentation
///
/// Load a File Record object with its ontent
///
/// ```lua
/// local file = utils.file.load("doc/README.md")
/// -- file.content contains the text content of the file
/// ```
///
/// ### Returns
///
///
/// ```lua
/// -- FileRecord
/// {
///   path    = "doc/README.md",
///   content = "... text content of the file ...",
///   name    = "README.md",
///   stem    = "README",
///   ext     = "md",
/// }
/// ```
///
///
pub(super) fn file_load(
	lua: &Lua,
	ctx: &RuntimeContext,
	rel_path: String,
	options: Option<Value>,
) -> mlua::Result<mlua::Value> {
	let base_path = compute_base_dir(ctx.dir_context(), options.as_ref())?;
	let rel_path = SPath::new(rel_path);

	let file_record = FileRecord::load(&base_path, &rel_path)?;
	let res = file_record.into_lua(lua)?;

	Ok(res)
}

/// ## Lua Documentation
///
/// Save a File Content into a path
///
/// ```lua
/// utils.file.save("doc/README.md", "Some very cool documentation")
/// ```
///
/// ### Returns
///
/// Does not return anything
///
pub(super) fn file_save(_lua: &Lua, ctx: &RuntimeContext, rel_path: String, content: String) -> mlua::Result<()> {
	let path = ctx.dir_context().resolve_path((&rel_path).into(), PathResolver::WksDir)?;
	ensure_file_dir(&path).map_err(Error::from)?;

	write(&path, content)?;

	get_hub().publish_sync(format!("-> Lua utils.file.save called on: {}", rel_path));

	Ok(())
}

/// ## Lua Documentation
///
/// Append content to a file at a specified path
///
/// ```lua
/// utils.file.append("doc/README.md", "Appended content to the file")
/// ```
///
/// ### Returns
///
/// Does not return anything
///
pub(super) fn file_append(_lua: &Lua, ctx: &RuntimeContext, rel_path: String, content: String) -> mlua::Result<()> {
	let path = ctx.dir_context().resolve_path((&rel_path).into(), PathResolver::WksDir)?;
	ensure_file_dir(&path).map_err(Error::from)?;

	let mut file = std::fs::OpenOptions::new()
		.append(true)
		.create(true)
		.open(&path)
		.map_err(Error::from)?;

	file.write_all(content.as_bytes())?;

	// NOTE: Could be too many prints
	// get_hub().publish_sync(format!("-> Lua utils.file.append called on: {}", rel_path));

	Ok(())
}

/// ## Lua Documentation
///
/// Ensure a file exists at the given path, and if not create it with an optional content
///
/// ```lua
/// utils.file.ensure_exists(path, optional_content) -- FileMeta
/// ```
///
/// ### Returns
///
/// Does not return anything
///
pub(super) fn file_ensure_exists(
	lua: &Lua,
	ctx: &RuntimeContext,
	path: String,
	content: Option<String>,
	options: Option<EnsureExistsOptions>,
) -> mlua::Result<mlua::Value> {
	let options = options.unwrap_or_default();
	let rel_path = SPath::new(path);
	let full_path = ctx.dir_context().resolve_path(rel_path.clone(), PathResolver::WksDir)?;

	// if the file does not exist, create it.
	if !full_path.exists() {
		simple_fs::ensure_file_dir(&full_path).map_err(|err| Error::custom(err.to_string()))?;
		let content = content.unwrap_or_default();
		write(&full_path, content)?;
	}
	// if we have the options.content_when_empty flag, if empty
	else if options.content_when_empty && files::is_file_empty(&full_path)? {
		let content = content.unwrap_or_default();
		write(full_path, content)?;
	}

	let file_meta = FileMeta::from(rel_path);

	file_meta.into_lua(lua)
}

/// ## Lua Documentation
///
/// List a set of file reference (no content) for a given glob
///
/// ```lua
/// let all_doc_file = utils.file.list("doc/**/*.md", options: {base_dir?: string, absolute?: bool})
/// ```
///
///
/// ### Returns
///
/// ```lua
/// -- An array/table of FileMeta
/// {
///   path    = "doc/README.md",
///   name    = "README.md",
///   stem    = "README",
///   ext     = "md"
/// }
/// ```
///
/// To get the content of files, needs iterate and load each
///
pub(super) fn file_list(
	lua: &Lua,
	ctx: &RuntimeContext,
	include_globs: Value,
	options: Option<Value>,
) -> mlua::Result<Value> {
	let (base_path, include_globs) = base_dir_and_globs(ctx, include_globs, options.as_ref())?;

	let absolute = options.x_get_bool("absolute").unwrap_or(false);

	let sfiles = list_files(
		&base_path,
		Some(&include_globs.x_as_strs()),
		Some(ListOptions::from_relative_glob(!absolute)),
	)
	.map_err(Error::from)?;

	// Now, we put back the paths found relative to base_path
	let sfiles = sfiles
		.into_iter()
		.map(|f| {
			if absolute {
				Ok(SPath::from(f))
			} else {
				//
				let diff = f.diff(&base_path)?;
				// if the diff goes back from base_path, then, we put the absolute path
				if diff.to_str().starts_with("..") {
					Ok(SPath::from(f))
				} else {
					Ok(diff)
				}
			}
		})
		.collect::<simple_fs::Result<Vec<SPath>>>()
		.map_err(|err| crate::Error::cc("Cannot list files to base", err))?;

	let file_metas: Vec<FileMeta> = sfiles.into_iter().map(FileMeta::from).collect();
	let res = file_metas.into_lua(lua)?;

	Ok(res)
}

/// ## Lua Documentation
///
/// List a set of file reference (no content) for a given glob and load them
///
/// ```lua
/// let all_doc_file = utils.file.list_load("doc/**/*.md", options: {base_dir?: string, absolute?: bool})
/// ```
///
///
/// ### Returns
///
/// ```lua
/// -- An array/table of FileRecord
/// {
///   path    = "doc/README.md",
///   name    = "README.md",
///   stem    = "README",
///   ext     = "md",
///   content = "..."
/// }
/// ```
///
/// To get the content of files, needs iterate and load each
///
pub(super) fn file_list_load(
	lua: &Lua,
	ctx: &RuntimeContext,
	include_globs: Value,
	options: Option<Value>,
) -> mlua::Result<Value> {
	let (base_path, include_globs) = base_dir_and_globs(ctx, include_globs, options.as_ref())?;

	let absolute = options.x_get_bool("absolute").unwrap_or(false);

	let sfiles = list_files(
		&base_path,
		Some(&include_globs.x_as_strs()),
		Some(ListOptions::from_relative_glob(!absolute)),
	)
	.map_err(Error::from)?;

	let file_records = sfiles
		.into_iter()
		.map(|sfile| -> Result<FileRecord> {
			if absolute {
				// Note the first path won't be taken in account by FileRecord (will need to make that better typed)
				let file_record = FileRecord::load(&SPath::from(""), &sfile.into())?;
				Ok(file_record)
			} else {
				//
				let diff = sfile.diff(&base_path)?;
				// if the diff goes back from base_path, then, we put the absolute path
				// TODO: need to double check this
				let (base_path, rel_path) = if diff.to_str().starts_with("..") {
					(SPath::from(""), SPath::from(sfile))
				} else {
					(base_path.clone(), diff)
				};
				let file_record = FileRecord::load(&base_path, &rel_path)?;
				Ok(file_record)
			}
		})
		.collect::<Result<Vec<_>>>()?;

	let res = file_records.into_lua(lua)?;

	Ok(res)
}

/// ## Lua Documentation
///
/// Return the first FileMeta or Nil
///
/// ```lua
/// let first_doc_file = utils.file.first("doc/**/*.md", options: {base_dir?: string, absolute?: bool})
/// ```
///
///
/// ### Returns
///
/// ```lua
/// -- FileMeta or Nil
/// {
///   path    = "doc/README.md",
///   name    = "README.md",
///   stem    = "README",
///   ext     = "md",
/// }
/// ```
///
/// To get the file record with .content, do
///
/// ```lua
/// let file = utils.file.load(file_meta.path)
/// ```
pub(super) fn file_first(
	lua: &Lua,
	ctx: &RuntimeContext,
	include_globs: Value,
	options: Option<Value>,
) -> mlua::Result<Value> {
	let (base_path, include_globs) = base_dir_and_globs(ctx, include_globs, options.as_ref())?;

	let absolute = options.x_get_bool("absolute").unwrap_or(false);

	let mut sfiles = iter_files(
		&base_path,
		Some(&include_globs.x_as_strs()),
		Some(ListOptions::from_relative_glob(!absolute)),
	)
	.map_err(Error::from)?;

	let Some(sfile) = sfiles.next() else {
		return Ok(Value::Nil);
	};

	let spath = if absolute {
		sfile.into()
	} else {
		sfile
			.diff(&base_path)
			.map_err(|err| Error::cc("Cannot diff with base_path", err))?
	};

	let res = FileMeta::from(spath).into_lua(lua)?;

	Ok(res)
}

// region:    --- Options
#[derive(Debug, Default)]
pub struct EnsureExistsOptions {
	/// Set the eventual provided content if the file is empty (only whitespaces)
	content_when_empty: bool,
}

impl FromLua for EnsureExistsOptions {
	fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
		let table = value
			.as_table()
			.ok_or(crate::Error::custom("EnsureExistsOptions should be a table"))?;
		let set_content_when_empty = table.get("content_when_empty")?;
		Ok(Self {
			content_when_empty: set_content_when_empty,
		})
	}
}

// endregion: --- Options

// region:    --- Support

/// return (base_path, globs)
fn base_dir_and_globs(
	ctx: &RuntimeContext,
	include_globs: Value,
	options: Option<&Value>,
) -> Result<(SPath, Vec<String>)> {
	let globs: Vec<String> = to_vec_of_strings(include_globs, "file::file_list globs argument")?;
	let base_dir = compute_base_dir(ctx.dir_context(), options)?;
	Ok((base_dir, globs))
}

fn compute_base_dir(dir_context: &DirContext, options: Option<&Value>) -> Result<SPath> {
	// the default base_path is the workspace dir.
	let workspace_path = dir_context.resolve_path("".into(), PathResolver::WksDir)?;

	// if options, try to resolve the options.base_dir
	let base_dir = get_value_prop_as_string(options, "base_dir", "utils.file... options fail")?;

	let base_dir = match base_dir {
		Some(base_dir) => {
			if paths::is_relative(&base_dir) {
				workspace_path.join(&base_dir)
			} else {
				SPath::from(base_dir)
			}
		}
		None => workspace_path,
	};

	Ok(base_dir)
}

// endregion: --- Support

// region:    --- Tests

#[cfg(test)]
mod tests {
	type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>; // For tests.

	use crate::_test_support::{SANDBOX_01_WKS_DIR, assert_contains, eval_lua, run_reflective_agent, setup_lua};
	use std::path::Path;
	use value_ext::JsonValueExt as _;

	#[tokio::test]
	async fn test_lua_file_load_simple_ok() -> Result<()> {
		// -- Setup & Fixtures
		let fx_path = "./agent-script/agent-hello.aip";

		// -- Exec
		let res = run_reflective_agent(&format!(r#"return utils.file.load("{fx_path}")"#), None).await?;

		// -- Check
		assert_contains(res.x_get_str("content")?, "from agent-hello.aip");
		assert_eq!(res.x_get_str("path")?, fx_path);
		assert_eq!(res.x_get_str("name")?, "agent-hello.aip");

		Ok(())
	}

	/// Note: need the multi-thread, because save do a `get_hub().publish_sync`
	///       which does a tokio blocking (requiring multi thread)
	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn test_lua_file_save_simple_ok() -> Result<()> {
		// -- Setup & Fixtures
		let fx_dest_path = "./.tmp/test_file_save_simple_ok/agent-hello.aip";
		let fx_content = "hello from test_file_save_simple_ok";

		// -- Exec
		let _res = run_reflective_agent(
			&format!(r#"return utils.file.save("{fx_dest_path}", "{fx_content}");"#),
			None,
		)
		.await?;

		// -- Check
		let dest_path = Path::new(SANDBOX_01_WKS_DIR).join(fx_dest_path);
		let file_content = std::fs::read_to_string(dest_path)?;
		assert_eq!(file_content, fx_content);

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_file_list_glob_direct() -> Result<()> {
		// -- Fixtures
		// This is the rust Path logic
		let glob = "*.*";

		// -- Exec
		let res = run_reflective_agent(&format!(r#"return utils.file.list("{glob}");"#), None).await?;

		// -- Check
		let res_paths = to_res_paths(&res);
		assert_eq!(res_paths.len(), 2, "result length");
		assert_contains(&res_paths, "file-01.txt");
		assert_contains(&res_paths, "file-02.txt");

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_file_list_glob_deep() -> Result<()> {
		// -- Fixtures
		// This is the rust Path logic
		let glob = "sub-dir-a/**/*.*";

		// -- Exec
		let res = run_reflective_agent(&format!(r#"return utils.file.list("{glob}");"#), None).await?;

		// -- Check
		let res_paths = to_res_paths(&res);
		assert_eq!(res_paths.len(), 3, "result length");
		assert_contains(&res_paths, "sub-dir-a/sub-sub-dir/agent-hello-3.aip");
		assert_contains(&res_paths, "sub-dir-a/sub-sub-dir/main.aip");
		assert_contains(&res_paths, "sub-dir-a/agent-hello-2.aip");

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_file_list_glob_abs_with_wild() -> Result<()> {
		// -- Fixtures
		let lua = setup_lua(super::super::init_module, "file")?;
		let dir = Path::new("./tests-data/config");
		let dir = dir
			.canonicalize()
			.map_err(|err| format!("Cannot canonicalize {dir:?} cause: {err}"))?;

		// This is the rust Path logic
		let glob = format!("{}/*.*", dir.to_string_lossy());
		let code = format!(r#"return utils.file.list("{glob}");"#);

		// -- Exec
		let _res = eval_lua(&lua, &code)?;

		// -- Check
		// TODO:

		Ok(())
	}

	#[test]
	fn test_lua_file_list_glob_with_base_dir_all_nested() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::super::init_module, "file")?;
		let lua_code = r#"
local files = utils.file.list({"**/*.*"}, {base_dir = "sub-dir-a"})
return { files = files }
		"#;

		// -- Exec
		let res = eval_lua(&lua, lua_code)?;

		// -- Check
		let files = res
			.get("files")
			.ok_or("Should have .files")?
			.as_array()
			.ok_or("file should be array")?;

		assert_eq!(files.len(), 3, ".files.len() should be 3");

		// NOTE: Here we assume the order will be deterministic and the same across OSes (tested on Mac).
		//       This logic might need to be changed, or actually, the list might need to have a fixed order.
		assert_eq!(
			"main.aip",
			files.first().ok_or("Should have a least one file")?.x_get_str("name")?
		);
		assert_eq!(
			"agent-hello-3.aip",
			files.get(1).ok_or("Should have a least two file")?.x_get_str("name")?
		);
		assert_eq!(
			"agent-hello-2.aip",
			files.get(2).ok_or("Should have a least two file")?.x_get_str("name")?
		);

		Ok(())
	}

	#[test]
	fn test_lua_file_list_glob_with_base_dir_one_level() -> Result<()> {
		// -- Setup & Fixtures
		let lua = setup_lua(super::super::init_module, "file")?;
		let lua_code = r#"
local files = utils.file.list({"agent-hello-*.aip"}, {base_dir = "sub-dir-a"})
return { files = files }
		"#;

		// -- Exec
		let res = eval_lua(&lua, lua_code)?;

		// -- Check
		let files = res
			.get("files")
			.ok_or("Should have .files")?
			.as_array()
			.ok_or("file should be array")?;

		assert_eq!(files.len(), 1, ".files.len() should be 1");
		// NOTE: Here we assume the order will be deterministic and the same across OSes (tested on Mac).
		//       This logic might need to be changed, or actually, the list might need to have a fixed order.
		assert_eq!(
			"agent-hello-2.aip",
			files.first().ok_or("Should have a least one file")?.x_get_str("name")?
		);

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_file_first_glob_deep() -> Result<()> {
		// -- Fixtures
		// This is the rust Path logic
		let glob = "sub-dir-a/**/*-2.*";

		// -- Exec
		let res = run_reflective_agent(&format!(r#"return utils.file.first("{glob}");"#), None).await?;

		// -- Check
		// let res_paths = to_res_paths(&res);
		assert_eq!(res.x_get_str("name")?, "agent-hello-2.aip");
		assert_eq!(res.x_get_str("path")?, "sub-dir-a/agent-hello-2.aip");

		Ok(())
	}

	#[tokio::test]
	async fn test_lua_file_first_not_found() -> Result<()> {
		// -- Fixtures
		// This is the rust Path logic
		let glob = "sub-dir-a/**/*-not-a-thing.*";

		// -- Exec
		let res = run_reflective_agent(&format!(r#"return utils.file.first("{glob}")"#), None).await?;

		// -- Check
		assert_eq!(res, serde_json::Value::Null, "Should have returned null");

		Ok(())
	}

	// region:    --- Support for Tests

	fn to_res_paths(res: &serde_json::Value) -> Vec<&str> {
		res.as_array()
			.ok_or("should have array of path")
			.unwrap()
			.iter()
			.map(|v| v.x_get_as::<&str>("path").unwrap_or_default())
			.collect::<Vec<&str>>()
	}

	// endregion: --- Support for Tests
}

// endregion: --- Tests
