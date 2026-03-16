use std::{borrow::Cow, collections::HashMap};

use anyhow::{anyhow, bail, Context};
use fancy_regex::Regex;
use serde::Deserialize;

use crate::{
	reformat::{self, ReformatPass},
	typespec,
};

#[derive(Deserialize, Debug)]
pub struct ModuleInfoHeader {
	pub name: String,
	pub description: String,
	#[serde(default)]
	pub headers: Vec<String>,
	#[serde(default)]
	pub qml_files: Vec<String>,
}

#[derive(Debug)]
pub struct ModuleInfo<'a> {
	pub header: ModuleInfoHeader,
	pub details: &'a str,
}

pub fn parse_module(text: &str) -> anyhow::Result<ModuleInfo> {
	let Some((mut header, mut details)) = text.split_once("-----") else {
		bail!("could not split module header");
	};

	header = header.trim();
	details = details.trim();

	let header =
		toml::from_str::<ModuleInfoHeader>(header).context("parsing module info header")?;

	Ok(ModuleInfo { header, details })
}

#[derive(Debug, Clone, Copy)]
pub struct Comment<'a> {
	pub text: &'a str,
	pub module: &'a str,
}

impl<'a> Comment<'a> {
	fn new(text: &'a str, module: &'a str) -> Self {
		Self { text, module }
	}
}

#[derive(Debug)]
pub struct ClassInfo<'a> {
	pub type_: ClassType,
	pub name: &'a str,
	pub qml_name: Option<&'a str>,
	pub superclass: Option<Cow<'a, str>>,
	pub singleton: bool,
	pub uncreatable: bool,
	pub comment: Option<Comment<'a>>,
	pub properties: Vec<Property<'a>>,
	pub invokables: Vec<Invokable<'a>>,
	pub signals: Vec<Signal<'a>>,
	pub enums: Vec<EnumInfo<'a>>,
}

#[derive(Debug)]
pub enum ClassType {
	Object,
	Gadget,
}

#[derive(Debug, Clone)]
pub struct Property<'a> {
	pub type_: Cow<'a, str>,
	pub name: &'a str,
	pub comment: Option<Comment<'a>>,
	pub readable: bool,
	pub writable: bool,
	pub default: bool,
}

#[derive(Debug, Clone)]
pub struct Invokable<'a> {
	pub name: &'a str,
	pub ret: &'a str,
	pub comment: Option<Comment<'a>>,
	pub params: Vec<InvokableParam<'a>>,
}

#[derive(Debug, Clone)]
pub struct Signal<'a> {
	pub name: &'a str,
	pub comment: Option<Comment<'a>>,
	pub params: Vec<InvokableParam<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct InvokableParam<'a> {
	pub name: &'a str,
	pub type_: &'a str,
}

#[derive(Debug)]
pub struct EnumInfo<'a> {
	pub namespace: &'a str,
	pub enum_name: &'a str,
	pub qml_name: &'a str,
	pub comment: Option<Comment<'a>>,
	pub variants: Vec<Variant<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct Variant<'a> {
	pub name: &'a str,
	pub comment: Option<Comment<'a>>,
}

pub struct CppParser {
	pub class_regex: Regex,
	pub macro_regex: Regex,
	pub property_regex: Regex,
	pub signals_regex: Regex,
	pub fn_regex: Regex,
	pub signal_regex: Regex,
	pub fn_param_regex: Regex,
	pub defaultprop_classinfo_regex: Regex,
	pub enum_ns_regex: Regex,
	pub enum_class_regex: Regex,
	pub enum_variant_regex: Regex,
}

pub struct QmlParser {
	pub class_regex: Regex,
	pub alias_regex: Regex,
	pub safe_body_regex: Regex,
	pub property_regex: Regex,
	pub function_regex: Regex,
}

#[derive(Debug)]
pub struct ParseContext<'a> {
	pub module: &'a str,
	pub classes: Vec<ClassInfo<'a>>,
	pub enums: Vec<EnumInfo<'a>>,
}

impl<'a> ParseContext<'a> {
	pub fn new(module: &'a str) -> Self {
		Self {
			module,
			classes: Vec::new(),
			enums: Vec::new(),
		}
	}
}

impl CppParser {
	pub fn new() -> Self {
		Self {
			class_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*class\s+(?<name>\w+)(?:\s*:\s*public\s+((?<super>\w+)(<.+>)?)(\s*,(\s*\w+)*)*)?\s*\{(?<body>[\s\S]*?)(?!};\s*Q_ENUM)};"#).unwrap(),
			macro_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*(?<hide>QSDOC_HIDE\s)?(?<type>(Q|QML|QSDOC)_\w+)\s*(\(\s*(?<args>.*)\s*\))?;"#).unwrap(),
			property_regex: Regex::new(r#"^\s*(?<type>(\w|::|, |<|>|\*)+)\*?\s+(?<name>\w+)(\s+(MEMBER\s+(?<member>\w+)|READ\s+(?<read>\w+)|WRITE\s+(?<write>\w+)|RESET\s+(?<reset>\w+)|NOTIFY\s+(?<notify>\w+)|BINDABLE\s+(?<bindable>\w+)|(?<const>CONSTANT)|(?<final>FINAL)))+\s*$"#).unwrap(),
			fn_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*(?<hide>QSDOC_HIDE\s)?(\/\/@\s+)?Q_INVOKABLE\s+(\[\[.*\]\]\s+)?(static\s+)?(virtual\s+)?(?<type>(\w|::|<|>)+\*?)\s+(?<name>\w+)\((?<params>[\s\S]*?)\)(\s*const)?\s*[;{]"#).unwrap(),
			signal_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*void\s+(?<name>\w+)\((?<params>[\s\S]*?)\);"#).unwrap(),
			fn_param_regex: Regex::new(r#"(const\s+)?(?<type>(\w|::|<|>)+\*?)&?\s+(?<name>\w+)(,|$)"#).unwrap(),
			signals_regex: Regex::new(r#"signals:(?<signals>(\s*(\s*///.*\s*)*void .*;)*)"#).unwrap(),
			defaultprop_classinfo_regex: Regex::new(r#"^\s*"DefaultProperty", "(?<prop>.+)"\s*$"#).unwrap(),
			enum_ns_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*namespace (?<namespace>\w+)\s*\{(?<nsbody>[\s\S]*?(QML_ELEMENT|QML_NAMED_ELEMENT\((?<qml_name>\w+)\));[\s\S]*?enum\s*(?<enum_name>\w+)\s*(:\s*\w+\s*)?\{(?<body>[\s\S]*?)\};[\s\S]*?)\}"#).unwrap(),
			enum_class_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*enum\s*(?<enum_name>\w+)\s*(:\s*\w+\s*)?\{(?<body>[\s\S]*?)\};\s+Q_ENUM\(.+\);"#).unwrap(),
			enum_variant_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*(?<name>\w+)\s*=\s*.+,"#).unwrap(),
		}
	}

	pub fn parse_classes<'a>(
		&self,
		text: &'a str,
		ctx: &mut ParseContext<'a>,
	) -> anyhow::Result<()> {
		for class in self.class_regex.captures_iter(text) {
			let class = class?;

			let comment = class.name("comment").map(|m| m.as_str());
			let mut name = class.name("name").unwrap().as_str();
			let mut superclass = class.name("super").map(|m| m.as_str());
			let body = class.name("body").unwrap().as_str();

			let mut classtype = None;
			let mut qml_name = None;
			let mut singleton = false;
			let mut uncreatable = false;
			let mut force_creatable = false;
			let mut properties = Vec::new();
			let mut default_property = None;
			let mut invokables = Vec::new();
			let mut notify_signals = Vec::new();
			let mut signals = Vec::new();
			let mut enums = Vec::new();

			struct Carryover<'a> {
				type_override: Option<&'a str>,
				comment: Option<&'a str>,
			}

			let mut carryover = Option::<Carryover<'a>>::None;

			(|| {
				for macro_ in self.macro_regex.captures_iter(body) {
					let macro_ = macro_?;

					if macro_.name("hide").is_some() {
						continue
					}

					let comment = macro_
						.name("comment")
						.map(|m| m.as_str())
						.or(carryover.as_ref().map(|c| c.comment).flatten());
					let type_ = macro_.name("type").unwrap().as_str();
					let args = macro_.name("args").map(|m| m.as_str());

					let this_carryover = carryover.take();

					(|| {
						match type_ {
							"QSDOC_BASECLASS" => {
								superclass = Some(args.expect(
									"QSDOC_BASECLASS must have the base class as an argument",
								))
							},
							"QSDOC_CNAME" => {
								name = args
									.expect("QSDOC_CNAME must specify the cname as an argument");
							},
							"Q_OBJECT" => classtype = Some(ClassType::Object),
							"Q_GADGET" => classtype = Some(ClassType::Gadget),
							"QML_ELEMENT" | "QSDOC_ELEMENT" => qml_name = Some(name),
							"QML_NAMED_ELEMENT" | "QSDOC_NAMED_ELEMENT" => {
								qml_name = Some(args.ok_or_else(|| {
									anyhow!("expected name for QML_NAMED_ELEMENT")
								})?)
							},
							"QML_SINGLETON" => singleton = true,
							"QML_UNCREATABLE" => uncreatable = true,
							"QSDOC_CREATABLE" => force_creatable = true,
							"QSDOC_TYPE_OVERRIDE" => {
								let type_override = args
									.ok_or_else(|| anyhow!("expected param for QSDOC_GENERIC"))?;

								carryover = Some(Carryover {
									type_override: Some(type_override),
									comment,
								});
							},
							"Q_PROPERTY" | "QSDOC_PROPERTY_OVERRIDE" => {
								let prop =
									self.property_regex
										.captures(args.ok_or_else(|| {
											anyhow!("expected args for Q_PROPERTY")
										})?)?
										.ok_or_else(|| anyhow!("unable to parse Q_PROPERTY"))?;

								let member = prop.name("member").is_some();
								let read = prop.name("read").is_some();
								let write = prop.name("write").is_some();
								let constant = prop.name("const").is_some();

								if let Some(notify) = prop.name("notify").map(|v| v.as_str()) {
									notify_signals.push(notify);
								}

								properties.push(Property {
									type_: Cow::Borrowed(
										this_carryover
											.as_ref()
											.map(|c| c.type_override)
											.flatten()
											.unwrap_or_else(|| prop.name("type").unwrap().as_str()),
									),
									name: prop.name("name").unwrap().as_str(),
									comment: comment.map(|v| Comment::new(v, ctx.module)),
									readable: read || member,
									writable: !constant && (write || member),
									default: false,
								});
							},
							"Q_CLASSINFO" => {
								let classinfo = self.defaultprop_classinfo_regex.captures(
									args.ok_or_else(|| anyhow!("expected args for Q_CLASSINFO"))?,
								)?;

								if let Some(classinfo) = classinfo {
									let prop = classinfo.name("prop").unwrap().as_str();
									default_property = Some(prop);
								}
							},
							_ => {},
						}
						Ok::<_, anyhow::Error>(())
					})()
					.with_context(|| {
						format!("while parsing macro `{}`", macro_.get(0).unwrap().as_str())
					})?;
				}

				if let Some(prop) = default_property {
					let prop = properties
						.iter_mut()
						.find(|p| p.name == prop)
						.ok_or_else(|| anyhow!("could not find default property `{prop}`"))?;

					prop.default = true;
				}

				for invokable in self.fn_regex.captures_iter(body) {
					let invokable = invokable?;

					if invokable.name("hide").is_some() {
						continue
					}

					let comment = invokable.name("comment").map(|m| m.as_str());
					let type_ = invokable.name("type").unwrap().as_str();
					let name = invokable.name("name").unwrap().as_str();
					let params_raw = invokable.name("params").unwrap().as_str();

					let mut params = Vec::new();

					for param in self.fn_param_regex.captures_iter(params_raw) {
						let param = param?;

						let type_ = param.name("type").unwrap().as_str();
						let name = param.name("name").unwrap().as_str();

						params.push(InvokableParam { type_, name });
					}

					invokables.push(Invokable {
						name,
						ret: type_,
						comment: comment.map(|v| Comment::new(v, ctx.module)),
						params,
					});
				}

				for signal_set in self.signals_regex.captures_iter(body) {
					let signal_set = signal_set?;

					let signals_body = signal_set.name("signals").unwrap().as_str();

					for signal in self.signal_regex.captures_iter(signals_body) {
						let signal = signal?;

						if signal.name("invokable").is_some() {
							continue;
						}

						let comment = signal.name("comment").map(|m| m.as_str());
						let name = signal.name("name").unwrap().as_str();
						let params_raw = signal.name("params").unwrap().as_str();

						if notify_signals.contains(&name) {
							continue;
						}

						let mut params = Vec::new();

						for param in self.fn_param_regex.captures_iter(params_raw) {
							let param = param?;

							let type_ = param.name("type").unwrap().as_str();
							let name = param.name("name").unwrap().as_str();

							params.push(InvokableParam { type_, name });
						}

						signals.push(Signal {
							name,
							comment: comment.map(|v| Comment::new(v, ctx.module)),
							params,
						});
					}
				}

				for enum_ in self.enum_class_regex.captures_iter(body) {
					let enum_ = enum_?;

					let comment = enum_.name("comment").map(|m| m.as_str());
					let enum_name = enum_.name("enum_name").unwrap().as_str();
					let body = enum_.name("body").unwrap().as_str();
					let variants = self.parse_enum_variants(body, ctx)?;

					enums.push(EnumInfo {
						namespace: name,
						enum_name,
						qml_name: enum_name,
						comment: comment.map(|v| Comment::new(v, ctx.module)),
						variants,
					});
				}

				Ok::<_, anyhow::Error>(())
			})()
			.with_context(|| format!("while parsing class `{name}`"))?;

			let Some(type_) = classtype else { continue };

			ctx.classes.push(ClassInfo {
				type_,
				name,
				qml_name,
				superclass: superclass.map(|s| Cow::Borrowed(s)),
				singleton,
				uncreatable: uncreatable && !force_creatable,
				comment: comment.map(|v| Comment::new(v, ctx.module)),
				properties,
				invokables,
				signals,
				enums,
			});
		}

		Ok(())
	}

	pub fn parse_enums<'a>(&self, text: &'a str, ctx: &mut ParseContext<'a>) -> anyhow::Result<()> {
		for enum_ in self.enum_ns_regex.captures_iter(text) {
			let enum_ = enum_?;

			let comment = enum_.name("comment").map(|m| m.as_str());
			let namespace = enum_.name("namespace").unwrap().as_str();
			let mut enum_name = enum_.name("enum_name").unwrap().as_str();
			let qml_name = enum_
				.name("qml_name")
				.map(|m| m.as_str())
				.unwrap_or(namespace);
			let nsbody = enum_.name("nsbody").unwrap().as_str();
			let body = enum_.name("body").unwrap().as_str();
			let variants = self.parse_enum_variants(body, ctx)?;

			for macro_ in self.macro_regex.captures_iter(nsbody) {
				let macro_ = macro_?;

				let type_ = macro_.name("type").unwrap().as_str();
				let args = macro_.name("args").map(|m| m.as_str());

				(|| {
					match type_ {
						"Q_DECLARE_FLAGS" => {
							enum_name = args
								.expect("Q_DECLARE_FLAGS must have arguments")
								.split_once(',')
								.expect("Q_DECLARE_FLAGS must have two arguments")
								.0
								.trim();
						},
						_ => {},
					}

					Ok::<_, anyhow::Error>(())
				})()
				.with_context(|| {
					format!("while parsing macro `{}`", macro_.get(0).unwrap().as_str())
				})?;
			}

			ctx.enums.push(EnumInfo {
				namespace,
				enum_name,
				qml_name,
				comment: comment.map(|v| Comment::new(v, ctx.module)),
				variants,
			});
		}

		Ok(())
	}

	pub fn parse_enum_variants<'a>(
		&self,
		body: &'a str,
		ctx: &ParseContext<'a>,
	) -> anyhow::Result<Vec<Variant<'a>>> {
		let mut variants = Vec::new();

		for variant in self.enum_variant_regex.captures_iter(body) {
			let variant = variant?;

			let comment = variant.name("comment").map(|m| m.as_str());
			let name = variant.name("name").unwrap().as_str();

			variants.push(Variant {
				name,
				comment: comment.map(|v| Comment::new(v, ctx.module)),
			});
		}

		Ok(variants)
	}

	pub fn parse<'a>(&self, text: &'a str, ctx: &mut ParseContext<'a>) -> anyhow::Result<()> {
		self.parse_classes(text, ctx)?;
		self.parse_enums(text, ctx)?;

		Ok(())
	}
}

// todo: use an actual parser (never)
impl QmlParser {
	pub fn new() -> Self {
		Self {
			class_regex: Regex::new(r#"((?<aliases>(\s*\/\/\/ alias.+)+)[\s\S]*?)?(?<comment>(\s*\/\/\/.*\n)+)?\s*(?<super>[A-Z]\w*)\s+{(?<body>[\s\S]*)}"#).unwrap(),
			alias_regex: Regex::new(r#"alias\s+(?<alias>\w+)\s+(?<definition>[\w.]+)"#).unwrap(),
			safe_body_regex: Regex::new(r#"((?<safebody1>^[\s\S]*?)(\n\s+[A-Z]\w* {[\s\S]*)|(?<safebody2>^[\s\S]*))"#).unwrap(),
			// note: can pick up function bodies
			property_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*(?<default>default\s+)?(?<required>required\s+)?(?<readonly>readonly\s+)?property\s+(\/\*(?<typeoverride>\w+)\*\/)?(?<type>\w+)\s+(?<name>\w+)\s*(:\s*(?<definition>(.*{\n[\s\S]*?\/\/ END-DEF|.*?(?!{)\n)))?"#).unwrap(),
			// note: can pick up prop bodies
			function_regex: Regex::new(r#"(?<comment>(\s*\/\/\/.*\n)+)?\s*function\s+(?<name>\w+)\s*\((?<params>.*)\)\s*:\s*(?<return>\w+)\s*{"#).unwrap(),
		}
	}

	pub fn parse<'a>(
		&self,
		filename: &'a str,
		text: &'a str,
		ctx: &mut ParseContext<'a>,
	) -> anyhow::Result<()> {
		for class in self.class_regex.captures_iter(text) {
			let class = class?;

			let mut aliases = HashMap::new();

			if let Some(aliases_str) = class.name("aliases").map(|m| m.as_str()) {
				for alias in self.alias_regex.captures_iter(aliases_str) {
					let alias = alias?;
					aliases.insert(
						alias.name("alias").unwrap().as_str(),
						alias.name("definition").unwrap().as_str(),
					);
				}
			}

			let alias_lookup = |name: &'a str| *aliases.get(name).unwrap_or(&name);

			let comment = class.name("comment").map(|m| m.as_str());
			let superclass = class.name("super").unwrap().as_str();

			let body = {
				let body = class.name("body").unwrap().as_str();
				let sbc = self
					.safe_body_regex
					.captures(body)?
					.ok_or_else(|| anyhow!("unable to capture safebody"))?;
				sbc.name("safebody1")
					.or_else(|| sbc.name("safebody2"))
					.unwrap()
					.as_str()
			};

			let mut properties = Vec::new();

			for prop in self.property_regex.captures_iter(body) {
				let prop = prop?;

				let type_ = prop
					.name("typeoverride")
					.or_else(|| prop.name("type"))
					.unwrap()
					.as_str();

				properties.push(Property {
					type_: Cow::Owned(format!("QML:{}", alias_lookup(type_))),
					name: prop.name("name").unwrap().as_str(),
					comment: prop
						.name("comment")
						.map(|m| Comment::new(m.as_str(), ctx.module)),
					readable: true,
					writable: prop.name("readonly").is_none(),
					default: prop.name("default").is_some(),
				});
			}

			let name = filename.split_once('.').unwrap().0;

			ctx.classes.push(ClassInfo {
				type_: ClassType::Object,
				name,
				qml_name: Some(name),
				superclass: Some(Cow::Owned(format!("QML:{}", superclass))),
				singleton: false,
				uncreatable: false,
				comment: comment.map(|v| Comment::new(v, ctx.module)),
				properties,
				invokables: Vec::new(),
				signals: Vec::new(),
				enums: Vec::new(),
			});
		}

		Ok(())
	}
}

impl ParseContext<'_> {
	pub fn gen_typespec(&self, module: &str) -> typespec::TypeSpec {
		typespec::TypeSpec {
			typemap: self
				.classes
				.iter()
				.flat_map(|class| {
					let Some(qmlname) = class.qml_name else { return Vec::new() };

					let mut classes = Vec::new();
					classes.push(typespec::QmlTypeMapping {
						// filters gadgets
						name: qmlname.to_string(),
						cname: class.name.to_string(),
						module: Some(module.to_string()),
					});

					// dirty hack to fix unknowns in resolution
					if let Some(e) = class.enums.iter().find(|e| e.enum_name == "Enum") {
						classes.push(typespec::QmlTypeMapping {
							// filters gadgets
							name: qmlname.to_string(),
							cname: format!("{}::{}", e.namespace, e.enum_name),
							module: Some(module.to_string()),
						});
					}

					classes
				})
				.collect(),
			classes: self
				.classes
				.iter()
				.filter_map(|class| {
					let (description, details) = class
						.comment
						.map(parse_details_desc)
						.unwrap_or((None, None));

					Some(typespec::Class {
						name: class.name.to_string(),
						module: module.to_string(),
						description,
						details,
						// filters gadgets
						superclass: class.superclass.clone()?.to_string(),
						singleton: class.singleton,
						uncreatable: class.uncreatable,
						properties: class.properties.iter().map(|p| p.clone().into()).collect(),
						functions: class.invokables.iter().map(|f| f.as_typespec()).collect(),
						signals: class.signals.iter().map(|s| s.as_typespec()).collect(),
						enums: class
							.enums
							.iter()
							.map(|enum_| {
								let (description, details) = enum_
									.comment
									.map(parse_details_desc)
									.unwrap_or((None, None));

								typespec::Enum {
									name: enum_.qml_name.to_string(),
									module: Some(module.to_string()),
									cname: Some(format!(
										"{}::{}",
										enum_.namespace, enum_.enum_name
									)),
									description,
									details,
									varaints: enum_.variants.iter().map(|v| (*v).into()).collect(),
								}
							})
							.collect(),
					})
				})
				.collect(),
			gadgets: self
				.classes
				.iter()
				.filter_map(|class| match class.type_ {
					ClassType::Gadget => Some(typespec::Gadget {
						cname: class.name.to_string(),
						properties: class.properties.iter().map(|p| p.clone().into()).collect(),
					}),
					_ => None,
				})
				.collect(),
			enums: self
				.enums
				.iter()
				.map(|enum_| {
					let (description, details) = enum_
						.comment
						.map(parse_details_desc)
						.unwrap_or((None, None));

					typespec::Enum {
						name: enum_.qml_name.to_string(),
						module: Some(module.to_string()),
						cname: Some(format!("{}::{}", enum_.namespace, enum_.enum_name)),
						description,
						details,
						varaints: enum_.variants.iter().map(|v| (*v).into()).collect(),
					}
				})
				.collect(),
		}
	}
}

impl From<Property<'_>> for typespec::Property {
	fn from(value: Property) -> Self {
		Self {
			type_: value.type_.to_string(),
			name: value.name.to_string(),
			details: value.comment.map(parse_details),
			readable: value.readable,
			writable: value.writable,
			default: value.default,
		}
	}
}

impl From<Variant<'_>> for typespec::Variant {
	fn from(value: Variant<'_>) -> Self {
		Self {
			name: value.name.to_string(),
			details: value.comment.map(parse_details),
		}
	}
}

impl Invokable<'_> {
	fn as_typespec(&self) -> typespec::Function {
		typespec::Function {
			ret: self.ret.to_string(),
			name: self.name.to_string(),
			details: self.comment.map(parse_details),
			params: self.params.iter().map(|p| (*p).into()).collect(),
		}
	}
}

impl Signal<'_> {
	fn as_typespec(&self) -> typespec::Signal {
		typespec::Signal {
			name: self.name.to_string(),
			details: self.comment.map(parse_details),
			params: self.params.iter().map(|p| (*p).into()).collect(),
		}
	}
}

impl From<InvokableParam<'_>> for typespec::FnParam {
	fn from(value: InvokableParam<'_>) -> Self {
		Self {
			type_: value.type_.to_string(),
			name: value.name.to_string(),
		}
	}
}

fn parse_details(comment: Comment) -> String {
	let mut seen_content = false;

	let mut str = comment
		.text
		.lines()
		.map(|line| {
			line.trim()
				.strip_prefix("///")
				.map(|line| line.strip_prefix(' ').unwrap_or(line))
				.unwrap_or(line)
		})
		.filter(|line| {
			let any = !line.is_empty();
			let filter = any || seen_content;
			seen_content |= any;
			filter
		})
		.fold(String::new(), |accum, line| accum + line.as_ref() + "\n");

	let reformat_ctx = reformat::Context {
		module: comment.module,
	};

	crate::reformat::GfmQuoteBlocks::new().reformat(&reformat_ctx, &mut str);
	crate::reformat::TypeLinks.reformat(&reformat_ctx, &mut str);

	str
}

fn parse_details_desc(comment: Comment) -> (Option<String>, Option<String>) {
	let details = parse_details(comment);
	if details.starts_with('!') {
		match details[1..].split_once('\n') {
			Some((desc, details)) => (
				Some(desc.strip_prefix(' ').unwrap_or(desc).to_string()),
				Some(details.to_string()),
			),
			None => (
				Some(
					details[1..]
						.strip_prefix(' ')
						.unwrap_or(&details[1..])
						.to_string(),
				),
				None,
			),
		}
	} else {
		(None, Some(details))
	}
}
