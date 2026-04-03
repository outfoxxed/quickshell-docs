use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TypeSpec {
	pub typemap: Vec<QmlTypeMapping>,
	pub classes: Vec<Class>,
	pub gadgets: Vec<Gadget>,
	pub enums: Vec<Enum>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QmlTypeMapping {
	pub name: String,
	pub cname: String,
	pub module: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Class {
	pub name: String,
	pub module: String,
	pub description: Option<String>,
	pub details: Option<String>,
	pub superclass: String,
	pub singleton: bool,
	pub uncreatable: bool,
	pub properties: Vec<Property>,
	pub functions: Vec<Function>,
	pub signals: Vec<Signal>,
	pub enums: Vec<Enum>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Gadget {
	pub cname: String,
	pub properties: Vec<Property>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Property {
	#[serde(rename = "type")]
	pub type_: String,
	pub name: String,
	pub details: Option<String>,
	pub readable: bool,
	pub writable: bool,
	pub default: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
	pub ret: String,
	pub name: String,
	pub details: Option<String>,
	pub params: Vec<FnParam>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Signal {
	pub name: String,
	pub details: Option<String>,
	pub params: Vec<FnParam>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FnParam {
	#[serde(rename = "type")]
	pub type_: String,
	pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Enum {
	pub name: String,
	pub cname: Option<String>,
	pub module: Option<String>,
	pub description: Option<String>,
	pub details: Option<String>,
	pub varaints: Vec<Variant>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Variant {
	pub name: String,
	pub details: Option<String>,
}
