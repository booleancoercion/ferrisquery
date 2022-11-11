pub struct Interface {
    address: String,
    password: String,
}

impl Interface {
    pub fn new(addr: impl ToString, pass: impl ToString) -> Self {
        Self {
            address: addr.to_string(),
            password: pass.to_string(),
        }
    }
}
