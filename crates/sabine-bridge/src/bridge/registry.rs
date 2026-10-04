/// A command pages may call, and where they may call it from.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeCommandDescriptor {
    pub name: String,
    /// Origins allowed to call only this command, in addition to the app's
    /// own pages and its allowed origins.
    pub allowed_origins: Vec<String>,
    /// Targets the command runs on, such as `desktop`, `linux`, `windows` or
    /// `macos`. A command without targets runs everywhere.
    pub targets: Vec<String>,
}

impl BridgeCommandDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            allowed_origins: Vec::new(),
            targets: Vec::new(),
        }
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.targets.push(target.into());
        self
    }

    pub fn allowed_origin(mut self, origin: impl Into<String>) -> Self {
        self.allowed_origins.push(origin.into());
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct BridgeRegistry {
    commands: Vec<BridgeCommandDescriptor>,
}

impl BridgeRegistry {
    pub fn register(&mut self, command_name: impl Into<String>) {
        self.register_descriptor(BridgeCommandDescriptor::new(command_name));
    }

    pub fn register_descriptor(&mut self, command: BridgeCommandDescriptor) {
        if !self
            .commands
            .iter()
            .any(|existing| existing.name == command.name)
        {
            self.commands.push(command);
        }
    }

    pub fn descriptor(&self, command_name: &str) -> Option<&BridgeCommandDescriptor> {
        self.commands
            .iter()
            .find(|command| command.name == command_name)
    }

    pub fn descriptors(&self) -> &[BridgeCommandDescriptor] {
        &self.commands
    }

    pub fn commands(&self) -> Vec<String> {
        self.commands
            .iter()
            .map(|command| command.name.clone())
            .collect()
    }
}
