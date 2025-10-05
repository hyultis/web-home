use serde::{Deserialize, Serialize};

#[derive(Debug,Serialize,Deserialize)]
enum Layout {
	Horizontal3Col(Vec<String>,Vec<String>,Vec<String>),
}