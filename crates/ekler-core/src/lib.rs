pub mod accounting;
pub mod error;
pub mod exhibits_list;
pub mod image;
pub mod model;
pub mod optimizer;
pub mod pdf;
pub mod pipeline;
pub mod plan;
pub mod safe_io;
pub use plan::{execute_export_plan, prepare_export_plan, split_into_new_exhibit, ExportPlan};
pub mod scanner;
pub mod toolbox;
pub mod udf;
pub mod validation;

pub use accounting::{
    calculate_sha256, verify_page_accounting, verify_source_integrity, PageAccountingReport,
};
pub use error::{EklerError, Result};
pub use exhibits_list::ExhibitsListGenerator;
pub use model::*;
pub use optimizer::{optimize_pdf, OptimizationLevel, OptimizationResult};
pub use pdf::{load_pdf_tolerant, TolerantLoadResult};
pub use pipeline::{execute_uyap_preparation, ExecutionContext, PipelineResult};
pub use scanner::{scan_source_files, ScanBatchResult, ScanFileError};
pub use toolbox::{
    delete_pages_from_file, detect_likely_blank_pages, extract_pages_to_file, images_to_pdf_file,
    merge_pdf_files, rotate_pdf_pages,
};
pub use udf::{convert_udf_to_markdown, inspect_udf, UdfInfo};
pub use validation::{validate_project_and_outputs, CheckItem, CheckLevel, ValidationReport};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod persistence;

pub mod office;

pub mod raster;
