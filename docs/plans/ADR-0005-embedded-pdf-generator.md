# ADR-0005: Embedded RTL Vector Arabic PDF Generator (`sf-report`)

- **Status**: Accepted
- **Context**: Specification Sections 4.11 and 17 require instant, offline Arabic vector PDF generation for data quality and cleaning reports, adhering strictly to Hard Rule #7 (Zero network access in engine crates). External crate dependencies like standard `typst` pull in heavy network-bound or non-WASM-compatible transitive dependencies that violate workspace architecture and WASM bundle size limits.
- **Decision**: Implement a self-contained, embedded Typst markup and vector Arabic PDF generator in `crates/sf-report/src/pdf.rs` with contextual Arabic Presentation Forms-B shaping (`ARABIC_SHAPING_TABLE` and Lam-Alef ligatures), direct PDF stream generation (`%PDF-1.7`, `Identity-H` encoding, RTL viewer preferences, vector score bar), guaranteeing fully offline execution in under 100ms and zero network overhead.
- **Consequences**: Complete compliance with Hard Rule #7, sub-100ms PDF generation in Web Workers and native Tauri/CLI, zero external vulnerabilities, and flawless RTL Arabic vector rendering in PDF viewers.
