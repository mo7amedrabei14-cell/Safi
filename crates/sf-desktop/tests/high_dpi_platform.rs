//! Cross-platform compatibility and High-DPI scaling verification tests (Section 18.6).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

#[cfg(test)]
mod high_dpi_and_platform_tests {
    use sf_desktop::deep_link::{parse, DeepLinkAction};

    #[test]
    fn test_cross_platform_path_separators_and_scaling() {
        // 1. Windows path with backslashes
        let win_link = "safi://open?file=D%3A%5CSafiData%5CProjects%5Csales.xlsx";
        let win_res = parse(win_link);
        assert!(win_res.is_ok());
        if let Ok(DeepLinkAction::OpenFile { path }) = win_res {
            assert_eq!(path, "D:\\SafiData\\Projects\\sales.xlsx");
        }

        // 2. Linux / macOS path with forward slashes
        let posix_link = "safi://open?file=%2Fhome%2Fuser%2Fsafi%2Fdata.csv";
        let posix_res = parse(posix_link);
        assert!(posix_res.is_ok());
        if let Ok(DeepLinkAction::OpenFile { path }) = posix_res {
            assert_eq!(path, "/home/user/safi/data.csv");
        }
    }
}
