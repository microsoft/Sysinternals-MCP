fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        // Re-run build script if VERSION env var changes
        println!("cargo:rerun-if-env-changed=VERSION");

        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "Sysinternals MCP Server");
        res.set(
            "FileDescription",
            "Sysinternals MCP Server - Windows system tools for AI assistants",
        );
        res.set("LegalCopyright", "Copyright \u{00a9} Microsoft");
        res.set("CompanyName", "Microsoft Corporation");
        res.set("OriginalFilename", "sysinternals-mcp.exe");

        // Use VERSION env var if set, otherwise fall back to Cargo.toml version
        if let Ok(version) = std::env::var("VERSION") {
            res.set("ProductVersion", &version);
            res.set("FileVersion", &version);

            // Parse version string into numeric format for Win32 VERSIONINFO
            let parts: Vec<u64> = version
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
            let numeric = (parts.first().copied().unwrap_or(0) << 48)
                | (parts.get(1).copied().unwrap_or(0) << 32)
                | (parts.get(2).copied().unwrap_or(0) << 16)
                | parts.get(3).copied().unwrap_or(0);
            res.set_version_info(winresource::VersionInfo::FILEVERSION, numeric);
            res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, numeric);
        }

        res.compile().expect("Failed to compile Windows resource");
    }
}
