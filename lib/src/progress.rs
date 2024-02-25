pub enum ProgressEvent {
    Output(String),
    DownloadPackage(String),
    InstallPackage(String),
    DownloadSource(String),
    Build(String, u64, u64),
    Action(String),
    CompressChest(u64, u64),
    ExtractChest(u64, u64),
}
