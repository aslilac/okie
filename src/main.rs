use anyhow::anyhow;
use colored::Colorize;
use once_cell::sync::Lazy;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::task;

mod groups;
mod options;
use options::Options;

const BASE: Lazy<reqwest::Url> = Lazy::new(|| {
	reqwest::Url::parse("https://raw.githubusercontent.com/aslilac/okie/main/static/")
		.expect("invalid base URL")
});

#[derive(Clone, Debug)]
struct Context {
	name: String,
}

fn parse_file_name(file: &str) -> anyhow::Result<(&str, reqwest::Url)> {
	let (file_path, tag) = file
		.rsplit_once("@")
		.map(|(file_path, tag)| (file_path, Some(tag)))
		.unwrap_or((&file, None));

	let mut base = BASE.clone();
	if let Some(tag) = tag {
		base = base.join(&format!("@{}/", tag))?;
	}

	Ok((file_path, base.join(&file_path)?))
}

async fn fetch_file<C>(file: &str, ctx: C) -> anyhow::Result<()>
where
	C: AsRef<Context>,
{
	let ctx = ctx.as_ref();
	let (file_path, url) = parse_file_name(file)?;

	// Fetch file, fill in template variables
	let file_content = reqwest::get(url)
		.await?
		.error_for_status()?
		.text()
		.await?
		.replace("{{name}}", &ctx.name);

	// Create parent directories as necessary
	if let Some(parent) = Path::new(&file).parent() {
		if !parent.exists() {
			fs::create_dir_all(parent)?;
		}
	}

	let file_path = file_path.replace("$name", &ctx.name);
	fs::write(file_path, file_content)?;

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let options = env::args().skip(1).collect::<Options>();

	let name = env::current_dir()?
		.file_name()
		.ok_or_else(|| anyhow!("expected current directory to have a name"))?
		.to_string_lossy()
		.to_string();

	let ctx = Arc::new(Context { name });

	let mut tasks = task::JoinSet::new();
	for file in options.files {
		let ctx = ctx.clone();
		tasks.spawn(async move {
			_ = fetch_file(&file, ctx)
				.await
				.map_err(|err| eprintln!("{} {}", "error:".red(), err));
		});
	}

	while !tasks.is_empty() {
		tasks.join_next().await.expect("task failed to execute")?;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_file_name() {
		let (file_path, url) = parse_file_name("Cargo.toml").unwrap();
		assert_eq!(file_path, "Cargo.toml");
		assert_eq!(url, BASE.join("Cargo.toml").unwrap());

		let (file_path, url) = parse_file_name("Cargo.toml@rust").unwrap();
		assert_eq!(file_path, "Cargo.toml");
		assert_eq!(url, BASE.join("@rust/Cargo.toml").unwrap());
	}
}
