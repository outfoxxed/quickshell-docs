use std::borrow::Cow;

use fancy_regex::Regex;

pub struct Context<'a> {
	pub module: &'a str,
}

pub trait ReformatPass {
	fn reformat(&self, context: &Context, text: &mut String);
}

pub struct GfmQuoteBlocks {
	callout_regex: Regex,
}

impl GfmQuoteBlocks {
	pub fn new() -> Self {
		Self {
			callout_regex: Regex::new(r#">\s+\[!(?<type>\w+)]\s+"#).unwrap(),
		}
	}
}

impl ReformatPass for GfmQuoteBlocks {
	fn reformat(&self, _: &Context, text: &mut String) {
		*text = text.replace("> [!INFO]", "> [!NOTE]");
		*text = self
			.callout_regex
			.replace_all(text, "> [!$type]\n> ")
			.to_string();
	}
}

pub struct TypeLinks;

impl ReformatPass for TypeLinks {
	fn reformat(&self, context: &Context, text: &mut String) {
		let lines = text.lines().map(|line| {
			if line.contains("@@") {
				let mut src: &str = line;
				let mut accum = String::new();

				while let Some(i) = src.find("@@") {
					accum += &src[..i];
					src = &src[i + 2..];

					let separators = [
						('$', true),
						(' ', false),
						(',', false),
						(';', false),
						(':', false),
					];

					let (mut end, mut ty) = src
						.chars()
						.enumerate()
						.find_map(|(i, char)| {
							separators
								.iter()
								.find(|(sc, _)| char == *sc)
								.map(|(_, strip)| (i + if *strip { 1 } else { 0 }, &src[..i]))
						})
						.unwrap_or((src.len(), src));

					// special case for . as it is contained in valid types as well
					if ty.ends_with('.') {
						end -= 1;
						ty = &ty[..ty.len() - 1];
					}

					let (ty, member) = match ty.chars().next() {
						None => (None, None),
						Some(c) if c.is_lowercase() => (None, Some(ty)),
						Some(_) => {
							let mut split = ty.rsplit_once('.').unwrap_or(("", ty));

							let member = if split.1.chars().next().unwrap().is_lowercase() {
								{
									let prop = split.1;
									split = split.0.rsplit_once('.').unwrap_or(("", split.0));
									prop
								}
							} else {
								""
							};

							let (mut module, name) = split;

							if module.is_empty() {
								module = context.module;
							}

							(Some((module, name)), Some(member))
						},
					};

					let (membertype, membername) = match member {
						None => ("", ""),
						Some(name) if name.ends_with("()") => ("func", &name[..name.len() - 2]),
						Some(name) if name.ends_with("(s)") => ("signal", &name[..name.len() - 3]),
						Some(name) if name.is_empty() => ("", ""),
						Some(name) => ("prop", name),
					};

					accum += "TYPE";

					if let Some((module, name)) = ty {
						if module.starts_with("Quickshell") {
							accum += "99MQS";
						} else {
							accum += "99MQT_qml";
						}

						accum = module
							.split('.')
							.fold(accum, |accum, next| accum + "_" + next)
							+ "99N" + name;
					}

					if !membername.is_empty() {
						accum += &format!("99V{membername}99T{membertype}");
					}

					accum += "99TYPE";
					src = &src[end..];
				}

				accum += src;

				Cow::Owned(accum)
			} else {
				Cow::Borrowed(line)
			}
		});

		*text = lines.fold(String::new(), |accum, line| accum + line.as_ref() + "\n");
	}
}
