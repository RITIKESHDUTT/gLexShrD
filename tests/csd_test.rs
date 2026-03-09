use gLexShrD::Glex;

#[cfg(test)]
#[test]
pub fn initiate() {
	Glex::app("GlexShrD Grapics Engine", 800, 600).expect("App creation Failed");
}