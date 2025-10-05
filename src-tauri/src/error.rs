use std::fmt::{Display, Formatter};
use tauri_plugin_updater::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdateError {
    UpdateNotFound = 1,
    EmptyEndpoints = 2,
    Io = 3,
    Semver = 4,
    Serialization = 5,
    ReleaseNotFound = 6,
    UnsupportedArch = 7,
    UnsupportedOs = 8,
    FailedToDetermineExtractPath = 9,
    UrlParse = 10,
    Reqwest = 11,
    TargetNotFound = 12,
    Network = 13,
    Minisign = 14,
    Base64 = 15,
    SignatureUtf8 = 16,
    TempDirNotOnSameMountPoint = 17,
    BinaryNotFoundInArchive = 18,
    TempDirNotFound = 19,
    AuthenticationFailed = 20,
    DebInstallFailed = 21,
    InvalidUpdaterFormat = 22,
    Http = 23,
    InvalidHeaderValue = 24,
    InvalidHeaderName = 25,
    FormatDate = 26,
    InsecureTransportProtocol = 27,
    Tauri = 28,
    Other = 99,
}

impl Display for UpdateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error performing self-update: {}", *self as i32)
    }
}

impl From<tauri_plugin_updater::Error> for UpdateError {
    fn from(err: Error) -> Self {
        match err {
            Error::EmptyEndpoints => UpdateError::EmptyEndpoints,
            Error::Io(_) => UpdateError::Io,
            Error::Semver(_) => UpdateError::Semver,
            Error::Serialization(_) => UpdateError::Serialization,
            Error::ReleaseNotFound => UpdateError::ReleaseNotFound,
            Error::UnsupportedArch => UpdateError::UnsupportedArch,
            Error::UnsupportedOs => UpdateError::UnsupportedOs,
            Error::FailedToDetermineExtractPath => UpdateError::FailedToDetermineExtractPath,
            Error::UrlParse(_) => UpdateError::UrlParse,
            Error::Reqwest(_) => UpdateError::Reqwest,
            Error::TargetNotFound(_) => UpdateError::TargetNotFound,
            Error::Network(_) => UpdateError::Network,
            Error::Minisign(_) => UpdateError::Minisign,
            Error::Base64(_) => UpdateError::Base64,
            Error::SignatureUtf8(_) => UpdateError::SignatureUtf8,
            Error::TempDirNotOnSameMountPoint => UpdateError::TempDirNotOnSameMountPoint,
            Error::BinaryNotFoundInArchive => UpdateError::BinaryNotFoundInArchive,
            Error::TempDirNotFound => UpdateError::TempDirNotFound,
            Error::AuthenticationFailed => UpdateError::AuthenticationFailed,
            Error::DebInstallFailed => UpdateError::DebInstallFailed,
            Error::InvalidUpdaterFormat => UpdateError::InvalidUpdaterFormat,
            Error::Http(_) => UpdateError::Http,
            Error::InvalidHeaderValue(_) => UpdateError::InvalidHeaderValue,
            Error::InvalidHeaderName(_) => UpdateError::InvalidHeaderName,
            Error::FormatDate => UpdateError::FormatDate,
            Error::InsecureTransportProtocol => UpdateError::InsecureTransportProtocol,
            Error::Tauri(_) => UpdateError::Tauri,
            _ => UpdateError::Other,
        }
    }
}
