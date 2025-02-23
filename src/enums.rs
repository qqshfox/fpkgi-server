#[derive(Debug, PartialEq)]
pub enum DRMCategory {
    None,
    PS4,
}

#[derive(Debug, PartialEq)]
pub enum ContentCategory {
    Game,
    DLC,
    App,
    Demo,
}

#[derive(Debug, PartialEq)]
pub enum IROCategory {
    SFTheme,
    SysTheme,
}
