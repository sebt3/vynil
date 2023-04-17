use crate::{Distrib, Error, Install};
use kube::ResourceExt;
use prometheus::{
    register_histogram_vec, register_int_counter, register_int_counter_vec, HistogramVec, IntCounter,
    IntCounterVec,
};
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub dist_reconciliations: IntCounter,
    pub dist_failures: IntCounterVec,
    pub dist_reconcile_duration: HistogramVec,
    pub inst_reconciliations: IntCounter,
    pub inst_failures: IntCounterVec,
    pub inst_reconcile_duration: HistogramVec,
}

impl Default for Metrics {
    fn default() -> Self {
        let dist_reconcile_duration = register_histogram_vec!(
            "dist_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();
        let dist_failures = register_int_counter_vec!(
            "dist_controller_reconciliation_errors_total",
            "reconciliation errors",
            &["instance", "error"]
        )
        .unwrap();
        let dist_reconciliations =
            register_int_counter!("dist_controller_reconciliations_total", "reconciliations").unwrap();
        let inst_reconcile_duration = register_histogram_vec!(
            "inst_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();
        let inst_failures = register_int_counter_vec!(
            "inst_controller_reconciliation_errors_total",
            "reconciliation errors",
            &["instance", "error"]
        )
        .unwrap();
        let inst_reconciliations =
            register_int_counter!("inst_controller_reconciliations_total", "reconciliations").unwrap();
        Metrics {
            dist_reconciliations,
            dist_failures,
            dist_reconcile_duration,
            inst_reconciliations,
            inst_failures,
            inst_reconcile_duration,
        }
    }
}

impl Metrics {
    pub fn dist_reconcile_failure(&self, dist: &Distrib, e: &Error) {
        self.dist_failures
            .with_label_values(&[dist.name_any().as_ref(), e.metric_label().as_ref()])
            .inc();
    }

    #[must_use] pub fn dist_count_and_measure(&self) -> ReconcileMeasurer {
        self.dist_reconciliations.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.dist_reconcile_duration.clone(),
        }
    }
    pub fn inst_reconcile_failure(&self, inst: &Install, e: &Error) {
        self.inst_failures
            .with_label_values(&[inst.name_any().as_ref(), e.metric_label().as_ref()])
            .inc();
    }

    #[must_use] pub fn inst_count_and_measure(&self) -> ReconcileMeasurer {
        self.inst_reconciliations.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.inst_reconcile_duration.clone(),
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
