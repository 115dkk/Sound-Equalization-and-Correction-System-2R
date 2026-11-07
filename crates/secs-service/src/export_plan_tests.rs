use std::sync::Arc;

use secs_project::MeasurementResult;
use secs_types::ipc::{
    CamillaDevices, Envelope, ErrorCode, ExportKind, ExportParams, MdatScope, ProjectNewParams,
    Request,
};

use crate::export_plan::ExportPurpose;
use crate::{Deps, NoEvents, Service, lock};

fn root(name: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "secs-export-plan-{}-{name}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn project(service: &Service, path: &std::path::Path, tutorial: bool) {
    let response = service.call(
        Request::ProjectNew(ProjectNewParams {
            path: if tutorial {
                String::new()
            } else {
                path.to_string_lossy().into_owned()
            },
            preset: "2.0".into(),
            sub_count: 0,
            sample: tutorial,
            tutorial,
        }),
        Arc::new(NoEvents),
    );
    assert!(matches!(response, Envelope::Ok(_)));
}

fn params(kind: ExportKind, folder: &std::path::Path) -> ExportParams {
    ExportParams {
        kind,
        folder: folder.to_string_lossy().into_owned(),
        mdat_scope: None,
        design_name: None,
        camilla_devices: None,
        extra_rates: Vec::new(),
    }
}

fn reason(error: secs_types::ipc::IpcError) -> String {
    assert!(matches!(
        error.code,
        ErrorCode::InvalidRequest | ErrorCode::TutorialBlocked
    ));
    error.params.get("reason").cloned().unwrap_or_default()
}

#[test]
fn plan_export_rejects_the_four_invalid_requests_with_reasons() {
    let root = root("invalid");
    let service = Service::new(Deps::default());
    project(&service, &root.join("room.secs"), false);

    let mut request = params(ExportKind::FilterText, std::path::Path::new("relative"));
    assert_eq!(
        reason(
            service
                .plan_export(request.clone(), ExportPurpose::Export)
                .unwrap_err()
        ),
        "relative_folder"
    );

    request = params(ExportKind::StagedMdat, &root.join("mdat"));
    request.design_name = Some("named".into());
    assert_eq!(
        reason(
            service
                .plan_export(request, ExportPurpose::Export)
                .unwrap_err()
        ),
        "design_name"
    );

    request = params(ExportKind::FilterText, &root.join("text"));
    request.camilla_devices = Some(CamillaDevices::default());
    assert_eq!(
        reason(
            service
                .plan_export(request, ExportPurpose::Export)
                .unwrap_err()
        ),
        "camilla_devices"
    );

    let tutorial = Service::new(Deps {
        app_data_path: root.join("app-data"),
        ..Deps::default()
    });
    project(&tutorial, std::path::Path::new(""), true);
    let error = tutorial
        .plan_export(
            params(ExportKind::FilterText, &root.join("tutorial")),
            ExportPurpose::Export,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::TutorialBlocked);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("tutorial")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plan_export_marks_apply_plans_without_index_or_record() {
    let root = root("apply");
    let service = Service::new(Deps::default());
    project(&service, &root.join("room.secs"), false);
    {
        let mut session = lock(&service.session);
        let open = session.require_mut().unwrap();
        open.store_mut()
            .record(
                MeasurementResult {
                    role: "L".into(),
                    pos: 1,
                    ir: vec![1.0, 0.0],
                    sample_rate: 48_000,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: Some("plan-test".into()),
                    source: None,
                    source_info: None,
                },
                Some(&["L".into(), "R".into()]),
            )
            .unwrap();
        open.mark_saved();
    }
    let mut request = params(ExportKind::BeforeAfterMdat, &root.join("mdat"));
    request.mdat_scope = Some(MdatScope::Measurements);
    let plan = service.plan_export(request, ExportPurpose::Apply).unwrap();
    assert!(plan.record.is_none());
    assert!(plan.destination.index_root.is_none());
    std::fs::remove_dir_all(root).unwrap();
}
