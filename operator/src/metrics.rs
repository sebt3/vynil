use crate::{Error, JukeBox, SystemInstance, TenantInstance};
use kube::ResourceExt;
use opentelemetry::trace::TraceId;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, exemplar::HistogramWithExemplars, family::Family},
    registry::{Registry, Unit},
};
use std::sync::Arc;
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub jukebox: ReconcileMetricsJukebox,
    pub system_instance: ReconcileMetricsSystemInstance,
    pub tenant_instance: ReconcileMetricsTenantInstance,
    pub reg_box: Arc<Registry>,
    pub reg_sys: Arc<Registry>,
    pub reg_tnt: Arc<Registry>,
}

impl Default for Metrics {
    fn default() -> Self {
        let mut reg_box = Registry::with_prefix("jukebox_reconcile");
        let mut reg_sys = Registry::with_prefix("system_instance_reconcile");
        let mut reg_tnt = Registry::with_prefix("tenant_instance_reconcile");
        let jukebox = ReconcileMetricsJukebox::default().register(&mut reg_box);
        let system_instance = ReconcileMetricsSystemInstance::default().register(&mut reg_sys);
        let tenant_instance = ReconcileMetricsTenantInstance::default().register(&mut reg_tnt);
        Self {
            reg_box: Arc::new(reg_box),
            reg_sys: Arc::new(reg_sys),
            reg_tnt: Arc::new(reg_tnt),
            jukebox,
            system_instance,
            tenant_instance,
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug, Default)]
pub struct TraceLabel {
    pub trace_id: String,
}
impl TryFrom<&TraceId> for TraceLabel {
    type Error = Error;

    fn try_from(id: &TraceId) -> Result<TraceLabel, Error> {
        if std::matches!(id, &TraceId::INVALID) {
            Err(Error::Other("Invalid trace ID".to_string()))
        } else {
            let trace_id = id.to_string();
            Ok(Self { trace_id })
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ErrorLabels {
    pub instance: String,
    pub error: String,
}


#[derive(Clone)]
pub struct ReconcileMetricsJukebox {
    pub runs: Counter,
    pub failures: Family<ErrorLabels, Counter>,
    pub duration: HistogramWithExemplars<TraceLabel>,
}

impl Default for ReconcileMetricsJukebox {
    fn default() -> Self {
        Self {
            runs: Counter::default(),
            failures: Family::<ErrorLabels, Counter>::default(),
            duration: HistogramWithExemplars::new([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter()),
        }
    }
}

impl ReconcileMetricsJukebox {
    /// Register API metrics to start tracking them.
    pub fn register(self, r: &mut Registry) -> Self {
        r.register_with_unit(
            "duration",
            "reconcile duration",
            Unit::Seconds,
            self.duration.clone(),
        );
        r.register("failures", "reconciliation errors", self.failures.clone());
        r.register("runs", "reconciliations", self.runs.clone());
        self
    }

    pub fn reconcile_failure(&self, doc: &JukeBox, e: &Error) {
        self.failures
            .get_or_create(&ErrorLabels {
                instance: doc.name_any(),
                error: e.metric_label(),
            })
            .inc();
    }

    pub fn count_and_measure(&self, trace_id: &TraceId) -> ReconcileMeasurer {
        self.runs.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            labels: trace_id.try_into().ok(),
            metric: self.duration.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ReconcileMetricsSystemInstance {
    pub runs: Counter,
    pub failures: Family<ErrorLabels, Counter>,
    pub duration: HistogramWithExemplars<TraceLabel>,
}

impl Default for ReconcileMetricsSystemInstance {
    fn default() -> Self {
        Self {
            runs: Counter::default(),
            failures: Family::<ErrorLabels, Counter>::default(),
            duration: HistogramWithExemplars::new([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter()),
        }
    }
}

impl ReconcileMetricsSystemInstance {
    /// Register API metrics to start tracking them.
    pub fn register(self, r: &mut Registry) -> Self {
        r.register_with_unit(
            "duration",
            "reconcile duration",
            Unit::Seconds,
            self.duration.clone(),
        );
        r.register("failures", "reconciliation errors", self.failures.clone());
        r.register("runs", "reconciliations", self.runs.clone());
        self
    }

    pub fn reconcile_failure(&self, doc: &SystemInstance, e: &Error) {
        self.failures
            .get_or_create(&ErrorLabels {
                instance: doc.name_any(),
                error: e.metric_label(),
            })
            .inc();
    }

    pub fn count_and_measure(&self, trace_id: &TraceId) -> ReconcileMeasurer {
        self.runs.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            labels: trace_id.try_into().ok(),
            metric: self.duration.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ReconcileMetricsTenantInstance {
    pub runs: Counter,
    pub failures: Family<ErrorLabels, Counter>,
    pub duration: HistogramWithExemplars<TraceLabel>,
}

impl Default for ReconcileMetricsTenantInstance {
    fn default() -> Self {
        Self {
            runs: Counter::default(),
            failures: Family::<ErrorLabels, Counter>::default(),
            duration: HistogramWithExemplars::new([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter()),
        }
    }
}

impl ReconcileMetricsTenantInstance {
    /// Register API metrics to start tracking them.
    pub fn register(self, r: &mut Registry) -> Self {
        r.register_with_unit(
            "duration",
            "reconcile duration",
            Unit::Seconds,
            self.duration.clone(),
        );
        r.register("failures", "reconciliation errors", self.failures.clone());
        r.register("runs", "reconciliations", self.runs.clone());
        self
    }

    pub fn reconcile_failure(&self, doc: &TenantInstance, e: &Error) {
        self.failures
            .get_or_create(&ErrorLabels {
                instance: doc.name_any(),
                error: e.metric_label(),
            })
            .inc();
    }

    pub fn count_and_measure(&self, trace_id: &TraceId) -> ReconcileMeasurer {
        self.runs.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            labels: trace_id.try_into().ok(),
            metric: self.duration.clone(),
        }
    }
}

/// Smart function duration measurer
///
/// Relies on Drop to calculate duration and register the observation in the histogram
pub struct ReconcileMeasurer {
    start: Instant,
    labels: Option<TraceLabel>,
    metric: HistogramWithExemplars<TraceLabel>,
}

impl Drop for ReconcileMeasurer {
    fn drop(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let duration = self.start.elapsed().as_millis() as f64 / 1000.0;
        let labels = self.labels.take();
        self.metric.observe(duration, labels);
    }
}
