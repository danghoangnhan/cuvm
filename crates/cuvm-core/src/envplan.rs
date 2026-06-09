/// OS-neutral activation intermediate; an `Activator` renders this per `Shell`.
/// Fields mirror the env-script contract in the spec (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPlan {
    pub cuda_home: String,
    pub cuda_path: String,
    pub toolkit_root: String,
    pub prepend_path: Vec<String>,
    pub prepend_lib: Vec<String>,
    pub current: String,
    pub injected: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envplan_holds_the_activation_fields() {
        let p = EnvPlan {
            cuda_home: "/home/u/.cuvm/versions/12.4.1".into(),
            cuda_path: "/home/u/.cuvm/versions/12.4.1".into(),
            toolkit_root: "/home/u/.cuvm/versions/12.4.1".into(),
            prepend_path: vec!["/home/u/.cuvm/versions/12.4.1/bin".into()],
            prepend_lib: vec!["/home/u/.cuvm/versions/12.4.1/lib64".into()],
            current: "12.4.1".into(),
            injected: vec![
                "/home/u/.cuvm/versions/12.4.1/bin".into(),
                "/home/u/.cuvm/versions/12.4.1/lib64".into(),
            ],
        };
        assert_eq!(p.cuda_home, p.cuda_path);
        assert_eq!(p.cuda_path, p.toolkit_root);
        assert_eq!(p.prepend_path.len(), 1);
        assert_eq!(p.injected.len(), 2);
        assert_eq!(p.current, "12.4.1");
        // Clone + PartialEq are part of the contract for golden tests.
        assert_eq!(p.clone(), p);
    }
}
