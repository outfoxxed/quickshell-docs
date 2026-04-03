use std::collections::HashMap;

use crate::{
	outform::{self, Flag, Parameter, PropertyType},
	typespec::{FnParam, Function, Property, Signal, TypeSpec},
};

pub fn resolve_types(
	module: &str,
	typespec: TypeSpec,
) -> anyhow::Result<HashMap<String, outform::TypeInfo>> {
	let mut outtypes = HashMap::new();

	let types = typespec
		.typemap
		.iter()
		.filter(|type_| type_.module.as_ref().map(|v| v as &str) == Some(module));

	let findqmltype = |name: &str| {
		if name.starts_with("QML:") {
			println!("QML? {name}");
			typespec
				.typemap
				.iter()
				.find(|type_| type_.name == name[4..])
		} else {
			typespec.typemap.iter().find(|type_| type_.cname == name)
		}
	};

	for mapping in types {
		let Some(class) = typespec
			.classes
			.iter()
			.find(|class| class.name == mapping.cname)
		else {
			continue
		};

		let mut properties = Vec::<&Property>::new();
		let mut functions = Vec::<&Function>::new();
		let mut signals = Vec::<&Signal>::new();

		// the first superclass availible from QML
		let mut superclass = &class.superclass;
		let superclass = loop {
			let type_ = findqmltype(superclass);

			if let Some(type_) = type_ {
				break outform::Type::resolve(type_.module.as_ref().map(|v| v as &str), &type_.name)
			}

			let superctype = typespec
				.classes
				.iter()
				.find(|class| &class.name == superclass);

			match superctype {
				Some(superctype) => {
					properties.extend(superctype.properties.iter());
					functions.extend(superctype.functions.iter());
					signals.extend(superctype.signals.iter());
					superclass = &superctype.superclass;
				},
				None => break outform::Type::unknown(),
			}
		};

		fn qmlparamtype(ctype: &str, typespec: &TypeSpec) -> outform::Type {
			if ctype.starts_with("QML:") {
				return match typespec
					.typemap
					.iter()
					.find(|type_| type_.name == ctype[4..])
				{
					Some(t) => {
						outform::Type::resolve(t.module.as_ref().map(|v| v as &str), &t.name)
					},
					None => outform::Type::unknown(),
				}
			}

			let (mut ctype, of) = match ctype.split_once('<') {
				None => (ctype, None),
				Some((ctype, mut remaining)) => {
					if remaining.ends_with('*') {
						remaining = &remaining[0..remaining.len() - 1];
					}

					// closing `>`
					remaining = &remaining[0..remaining.len() - 1];

					let of = Box::new(qmlparamtype(remaining, typespec));

					(ctype, Some(of))
				},
			};

			if ctype.ends_with('*') {
				ctype = &ctype[0..ctype.len() - 1];
			}

			// note: suffix is checked instead of == due to Q_PROPERTY using fully qualified names
			let qtype = typespec
				.typemap
				.iter()
				.find(|type_| !type_.cname.is_empty() && ctype.ends_with(&type_.cname))
				.map(|type_| (&type_.module, &type_.name))
				.or_else(|| {
					typespec
						.enums
						.iter()
						.find(|type_| {
							type_
								.cname
								.as_ref()
								.map(|v| v as &str)
								.map(|v| !v.is_empty() && ctype.ends_with(v))
								.unwrap_or(false)
						})
						.map(|type_| (&type_.module, &type_.name))
				});

			match qtype {
				Some((module, name)) => {
					let mut t = outform::Type::resolve(module.as_ref().map(|v| v as &str), name);
					t.of = of;
					t
				},
				None => outform::Type::unknown(),
			}
		}

		fn solveprop(prop: &Property, typespec: &TypeSpec) -> outform::Property {
			let ctype = &prop.type_[..];

			let flags = {
				let mut flags = Vec::new();

				if prop.default {
					flags.push(Flag::Default);
				}

				if !prop.readable {
					flags.push(Flag::Writeonly);
				} else if !prop.writable {
					flags.push(Flag::Readonly);
				}

				flags
			};

			let gadget = typespec.gadgets.iter().find(|gadget| gadget.cname == ctype);

			match gadget {
				Some(gadget) => outform::Property {
					type_: PropertyType::Gadget(
						gadget
							.properties
							.iter()
							.map(|prop| (prop.name.clone(), solveprop(prop, typespec).type_))
							.collect(),
					),
					details: prop.details.clone(),
					flags,
				},
				None => outform::Property {
					type_: PropertyType::Type(qmlparamtype(ctype, typespec)),
					details: prop.details.clone(),
					flags,
				},
			}
		}

		fn solvefunc(func: &Function, typespec: &TypeSpec) -> outform::Function {
			outform::Function {
				ret: qmlparamtype(&func.ret, typespec),
				name: func.name.clone(),
				id: func.name.clone(),
				details: func.details.clone(),
				params: func
					.params
					.iter()
					.map(|FnParam { type_, name }| Parameter {
						name: name.clone(),
						type_: qmlparamtype(type_, typespec),
					})
					.collect(),
			}
		}

		fn solvesignal(func: &Signal, typespec: &TypeSpec) -> outform::Signal {
			outform::Signal {
				name: func.name.clone(),
				details: func.details.clone(),
				params: func
					.params
					.iter()
					.map(|FnParam { type_, name }| Parameter {
						name: name.clone(),
						type_: qmlparamtype(type_, typespec),
					})
					.collect(),
			}
		}

		properties.extend(class.properties.iter());
		properties.sort_by(|a, b| Ord::cmp(&a.name, &b.name));

		functions.extend(class.functions.iter());
		functions.sort_by(|a, b| Ord::cmp(&a.name, &b.name));

		signals.extend(class.signals.iter());
		signals.sort_by(|a, b| Ord::cmp(&a.name, &b.name));

		let properties = properties
			.iter()
			.map(|prop| (prop.name.clone(), solveprop(prop, &typespec)))
			.collect::<HashMap<_, _>>();

		let functions = functions
			.iter()
			.map(|func| solvefunc(func, &typespec))
			.collect::<Vec<_>>();

		let signals = signals
			.iter()
			.map(|signal| (signal.name.clone(), solvesignal(signal, &typespec)))
			.collect::<HashMap<_, _>>();

		let coreenum = class.enums.iter().find(|e| e.name == "Enum");
		let variants = match coreenum {
			Some(e) => e
				.varaints
				.iter()
				.map(|variant| {
					(variant.name.clone(), outform::Variant {
						details: variant.details.clone(),
					})
				})
				.collect(),
			None => HashMap::new(),
		};

		let type_ = outform::TypeInfo {
			name: mapping.name.clone(),
			module: mapping.module.clone().unwrap(),
			details: outform::TypeDetails::Class(outform::ClassInfo {
				superclass,
				description: class.description.clone(),
				details: class.details.clone(),
				flags: {
					let mut flags = Vec::new();

					if coreenum.is_some() {
						flags.push(Flag::Enum);
					} else if class.singleton {
						flags.push(Flag::Singleton);
					} else if class.uncreatable {
						flags.push(Flag::Uncreatable);
					}

					flags
				},
				properties,
				functions,
				signals,
				variants,
			}),
		};

		outtypes.insert(mapping.name.clone(), type_);
	}

	for enum_ in typespec.enums {
		if enum_.module.as_ref().map(|v| v as &str) == Some(module) {
			outtypes.insert(enum_.name.clone(), outform::TypeInfo {
				name: enum_.name,
				module: enum_.module.unwrap(),
				details: outform::TypeDetails::Enum(outform::EnumInfo {
					description: enum_.description,
					details: enum_.details,
					variants: enum_
						.varaints
						.into_iter()
						.map(|variant| {
							(variant.name, outform::Variant {
								details: variant.details,
							})
						})
						.collect(),
				}),
			});
		}
	}

	Ok(outtypes)
}
