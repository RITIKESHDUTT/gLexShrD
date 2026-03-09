use gLexShrD::Glex;

#[cfg(test)]
#[test]
pub fn initiate() {
	Glex::app("GlexShrD Grapics Engine", 1680, 1440).expect("App creation Failed");
}