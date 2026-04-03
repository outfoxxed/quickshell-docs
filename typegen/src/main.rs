use std::{collections::HashMap, path::Path};

use anyhow::{anyhow, Context};
use walkdir::WalkDir;

mod outform;
mod parse;
mod reformat;
mod resolver;
mod typespec;

fn main() -> anyhow::Result<()> {
	let args = std::env::args().collect::<Vec<_>>();

	match args.get(1).map(|v| v as &str) {
		Some("fulltypegen") => {
			let path = args.get(2).expect("expected basedir");
			let outpath = args.get(3).expect("expected outpath");
			let datapath = args.get(4).expect("expected datapath");
			let templatepath = args.get(5).expect("expected templatepath");
			let extratypedirs = &args[6..];

			let module_files = WalkDir::new(path)
				.into_iter()
				.filter(|e| {
					e.as_ref()
						.map(|e| e.file_name() == "module.md")
						.unwrap_or(false)
				})
				.map(|entry| match entry {
					Ok(entry) => {
						let path = entry.path().to_string_lossy().to_string();
						let text = std::fs::read_to_string(&path)?;
						let module = parse::parse_module(&text)?;
						Ok((path, module.header))
					},
					Err(e) => Err(anyhow!(e)),
				})
				.collect::<Result<Vec<(String, parse::ModuleInfoHeader)>, anyhow::Error>>()?;

			println!("Generating types -> {outpath}");

			for (path, header) in module_files.iter() {
				let name = &header.name;
				let mod_outpath = format!("{outpath}/{name}.json");
				println!("Gentypes :: {path} ({name}) -> {mod_outpath}");
				gentypes(path, &mod_outpath)?;
			}

			let mut typefiledirs = extratypedirs.to_vec();
			typefiledirs.push(outpath.clone());

			println!("Generating docs {typefiledirs:?} -> {datapath}");

			// this is crap but I don't care, typegen is getting replaced
			let typefiles = typefiledirs
				.iter()
				.map(|dir| {
					let dirs = Path::new(dir)
						.read_dir()?
						.flatten()
						.map(|e| e.path().to_string_lossy().to_string())
						.collect::<Vec<String>>();
					Ok::<_, anyhow::Error>(dirs)
				})
				.collect::<Result<Vec<Vec<String>>, anyhow::Error>>()?
				.into_iter()
				.flatten()
				.collect::<Vec<String>>();

			for (path, header) in module_files.iter() {
				let name = &header.name;
				let mod_datapath = format!("{datapath}/{name}");
				let mod_templatepath = format!("{templatepath}/{name}");
				println!("Gendocs :: {path} ({name}) to {mod_datapath}");
				gendocs(path, &mod_datapath, &mod_templatepath, &typefiles[..])?;
			}
		},
		Some("gentypes") => {
			let modinfo = args.get(2).expect("expected module file");
			let outpath = args.get(3).expect("expected output path");
			gentypes(modinfo, outpath)?;
		},
		Some("gendocs") => {
			let modinfo = args.get(2).expect("expected module file");
			let datapath = args.get(3).expect("expected datapath");
			let templatepath = args.get(4).expect("expected templatepath");
			gendocs(modinfo, datapath, templatepath, &args[5..])?;
		},
		_ => {
			panic!("typegen invoked without mode");
		},
	}

	Ok(())
}

fn gentypes(modinfo: &str, outpath: &str) -> anyhow::Result<()> {
	let path = Path::new(modinfo);
	let dir = path.parent().unwrap();
	let text = std::fs::read_to_string(path).expect("failed to read module file");
	let module = parse::parse_module(&text)?;

	let texts = module
		.header
		.headers
		.iter()
		.map(|header| {
			let text = std::fs::read_to_string(dir.join(header)).with_context(|| {
				format!(
					"failed to read module header `{header}` at {:?}",
					dir.join(header)
				)
			})?;

			Ok::<_, anyhow::Error>((header, text))
		})
		.collect::<Result<HashMap<_, _>, _>>()?;

	let qml_texts = module
		.header
		.qml_files
		.iter()
		.map(|file| {
			let text = std::fs::read_to_string(dir.join(file)).with_context(|| {
				format!(
					"failed to read module qml file `{file}` at {:?}",
					dir.join(file)
				)
			})?;

			Ok::<_, anyhow::Error>((file, text))
		})
		.collect::<Result<HashMap<_, _>, _>>()?;

	let header_parser = parse::CppParser::new();
	let qml_parser = parse::QmlParser::new();
	let mut ctx = parse::ParseContext::new(&module.header.name);

	texts.iter().try_for_each(|(header, text)| {
		header_parser
			.parse(text, &mut ctx)
			.with_context(|| format!("while parsing module header `{header}`"))
	})?;

	qml_texts.iter().try_for_each(|(file, text)| {
		qml_parser
			.parse(file, text, &mut ctx)
			.with_context(|| format!("while parsing module qml file `{file}`"))
	})?;

	let typespec = ctx.gen_typespec(&module.header.name);

	let text = serde_json::to_string_pretty(&typespec).unwrap();

	std::fs::write(outpath, text).context("saving typespec")
}

fn gendocs(
	modinfo: &str,
	datapath: &str,
	templatepath: &str,
	typepaths: &[String],
) -> anyhow::Result<()> {
	let text = std::fs::read_to_string(modinfo).expect("failed to read module file");
	let module = parse::parse_module(&text)?;

	let mut typespec = typespec::TypeSpec::default();

	for path in typepaths {
		let text =
			std::fs::read_to_string(path).with_context(|| anyhow!("attempting to read {path}"))?;

		let ts = serde_json::from_str::<typespec::TypeSpec>(&text)
			.with_context(|| anyhow!("attempting to parse {path}"))?;

		typespec.typemap.extend(ts.typemap);
		typespec.classes.extend(ts.classes);
		typespec.gadgets.extend(ts.gadgets);
		typespec.enums.extend(ts.enums);
	}

	let types = resolver::resolve_types(&module.header.name, typespec)?;

	let datapath = Path::new(datapath);
	let templatepath = Path::new(templatepath);
	std::fs::create_dir_all(datapath)?;
	std::fs::create_dir_all(templatepath)?;

	for (name, info) in types {
		let json = serde_json::to_string_pretty(&info).unwrap();
		let datapath = datapath.join(format!("{name}.json"));
		std::fs::write(&datapath, json).with_context(|| format!("while writing {datapath:?}"))?;

		let template = format!(
			"+++
title = \"{name}\"
hidetitle = true
+++

{{{{< qmltype module=\"{module}\" type=\"{name}\" >}}}}
",
			name = name,
			module = module.header.name
		);

		let templatepath = templatepath.join(format!("{name}.md"));
		std::fs::write(&templatepath, template)
			.with_context(|| format!("while writing {templatepath:?}"))?;
	}

	let index = outform::ModuleIndex {
		name: module.header.name.to_string(),
		description: module.header.description,
		details: module.details.to_string(),
	};

	let datapath = datapath.join("index.json");
	let json = serde_json::to_string_pretty(&index).unwrap();
	std::fs::write(&datapath, json).with_context(|| format!("while writing {datapath:?}"))?;

	let template = format!(
		"+++
title = \"{name}\"
hidetitle = true
+++

{{{{< qmlmodule module=\"{name}\" >}}}}
",
		name = module.header.name
	);

	let templatepath = templatepath.join("_index.md");
	std::fs::write(&templatepath, template)
		.with_context(|| format!("while writing {templatepath:?}"))
}
