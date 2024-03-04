use std::collections::HashMap;

use crate::{
	outform::{self, Flag, PropertyType},
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

	let findqmltype = |cname: &str| typespec.typemap.iter().find(|type_| type_.cname == cname);

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
			let qtype = typespec
				.typemap
				.iter()
				.find(|type_| &type_.cname == ctype)
				.map(|type_| (&type_.module, &type_.name))
				.or_else(|| {
					typespec
						.enums
						.iter()
						.find(|type_| type_.cname.as_ref().map(|v| v as &str) == Some(ctype))
						.map(|type_| (&type_.module, &type_.name))
				});

			match qtype {
				Some((module, name)) => {
					outform::Type::resolve(module.as_ref().map(|v| v as &str), &name)
				},
				None => outform::Type::unknown(),
			}
		}

		fn solveprop(prop: &Property, typespec: &TypeSpec) -> outform::Property {
			let mut ctype = &prop.type_[..];

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
				None => {
					let mut list = false;

					if ctype.starts_with("QQmlListProperty<") {
						ctype = &ctype[17..ctype.len() - 1];
						list = true;
					} else if ctype.starts_with("QList<") {
						ctype = &ctype[6..ctype.len() - 1];
						list = true;
					}

					let mut type_ = qmlparamtype(ctype, typespec);

					if list {
						type_ = outform::Type {
							type_: outform::TypeSource::Qt,
							module: "qml".to_string(),
							name: "list".to_string(),
							of: Some(Box::new(type_)),
						};
					}

					outform::Property {
						type_: PropertyType::Type(type_),
						details: prop.details.clone(),
						flags,
					}
				},
			}
		}

		fn solvefunc(func: &Function, typespec: &TypeSpec) -> outform::Function {
			outform::Function {
				ret: qmlparamtype(&func.ret, typespec),
				name: func.name.clone(),
				details: func.details.clone(),
				params: func
					.params
					.iter()
					.map(|FnParam { type_, name }| (name.clone(), qmlparamtype(type_, typespec)))
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
					.map(|FnParam { type_, name }| (name.clone(), qmlparamtype(type_, typespec)))
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
			.map(|func| (func.name.clone(), solvefunc(func, &typespec)))
			.collect::<HashMap<_, _>>();

		let signals = signals
			.iter()
			.map(|signal| (signal.name.clone(), solvesignal(signal, &typespec)))
			.collect::<HashMap<_, _>>();

		let type_ = outform::ClassInfo {
			superclass,
			description: class.description.clone(),
			details: class.details.clone(),
			flags: {
				let mut flags = Vec::new();

				if class.singleton {
					flags.push(Flag::Singleton);
				} else if class.uncreatable {
					flags.push(Flag::Uncreatable);
				}

				flags
			},
			properties,
			functions,
			signals,
		};

		outtypes.insert(mapping.name.clone(), outform::TypeInfo::Class(type_));
	}

	for enum_ in typespec.enums {
		if enum_.module.as_ref().map(|v| v as &str) == Some(module) {
			outtypes.insert(
				enum_.name,
				outform::TypeInfo::Enum(outform::EnumInfo {
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
			);
		}
	}

	Ok(outtypes)
}
