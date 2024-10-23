use crate::{JukeBox, TenantInstance, SystemInstance, Error};
use kube::ResourceExt;
use prometheus::{
    register_histogram_vec, register_int_counter, register_int_counter_vec, HistogramVec, IntCounter,
    IntCounterVec,
};
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub jukebox_reconciliations: IntCounter,
    pub jukebox_failures: IntCounterVec,
    pub jukebox_reconcile_duration: HistogramVec,
    pub tenant_reconciliations: IntCounter,
    pub tenant_failures: IntCounterVec,
    pub tenant_reconcile_duration: HistogramVec,
    pub system_reconciliations: IntCounter,
    pub system_failures: IntCounterVec,
    pub system_reconcile_duration: HistogramVec,
}

impl Default for Metrics {
    fn default() -> Self {
        let jukebox_reconcile_duration = register_histogram_vec!(
            "jukebox_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();
        let jukebox_failures = register_int_counter_vec!(
            "jukebox_controller_reconciliation_errors_total",
            "reconciliation errors",
            &["instance", "error"]
        )
        .unwrap();
        let jukebox_reconciliations =
            register_int_counter!("jukebox_controller_reconciliations_total", "reconciliations").unwrap();
        let tenant_reconcile_duration = register_histogram_vec!(
            "tenant_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();
        let tenant_failures = register_int_counter_vec!(
            "tenant_controller_reconciliation_errors_total",
            "reconciliation errors",
            &["instance", "error"]
        )
        .unwrap();
        let tenant_reconciliations =
            register_int_counter!("tenant_controller_reconciliations_total", "reconciliations").unwrap();
        let system_reconcile_duration = register_histogram_vec!(
            "system_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();
        let system_failures = register_int_counter_vec!(
            "system_controller_reconciliation_errors_total",
            "reconciliation errors",
            &["instance", "error"]
        )
        .unwrap();
        let system_reconciliations =
            register_int_counter!("system_controller_reconciliations_total", "reconciliations").unwrap();
        Metrics {
            jukebox_reconciliations,
            jukebox_failures,
            jukebox_reconcile_duration,
            tenant_reconciliations,
            tenant_failures,
            tenant_reconcile_duration,
            system_reconciliations,
            system_failures,
            system_reconcile_duration,
        }
    }
}

impl Metrics {
    pub fn jukebox_reconcile_failure(&self, jukebox: &JukeBox, e: &Error) {
        self.jukebox_failures
            .with_label_values(&[jukebox.name_any().as_ref(), e.metric_label().as_ref()])
            .inc();
    }

    #[must_use] pub fn jukebox_count_and_measure(&self) -> ReconcileMeasurer {
        self.jukebox_reconciliations.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.jukebox_reconcile_duration.clone(),
        }
    }
    pub fn tenant_reconcile_failure(&self, inst: &TenantInstance, e: &Error) {
        self.tenant_failures
            .with_label_values(&[inst.name_any().as_ref(), e.metric_label().as_ref()])
            .inc();
    }

    #[must_use] pub fn tenant_count_and_measure(&self) -> ReconcileMeasurer {
        self.tenant_reconciliations.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.tenant_reconcile_duration.clone(),
        }
    }
    pub fn system_reconcile_failure(&self, inst: &SystemInstance, e: &Error) {
        self.system_failures
            .with_label_values(&[inst.name_any().as_ref(), e.metric_label().as_ref()])
            .inc();
    }

    #[must_use] pub fn system_count_and_measure(&self) -> ReconcileMeasurer {
        self.system_reconciliations.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.system_reconcile_duration.clone(),
        }
    }
}

pub struct ReconcileMeasurer {
    start: Instant,
    metric: HistogramVec,
}

impl Drop for ReconcileMeasurer {
    fn drop(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let duration = self.start.elapsed().as_millis() as f64 / 1000.0;
        self.metric.with_label_values(&[]).observe(duration);
    }
}
