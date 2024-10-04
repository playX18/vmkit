use std::{str::FromStr, sync::{atomic::Ordering, OnceLock}};

use atomic::Atomic;
use mmtk::{
    util::options::{GCTriggerSelector, NurserySize, PlanSelector},
    MMTKBuilder,
};
use parking_lot::Mutex;

use crate::{define_flag, define_option_handler, utils::MemorySize};

pub struct MMTKFlags;

define_option_handler!(MMTKFlags => parse_gc_plan, plan, "Select GC plan (default: GenImmix)");
define_flag!(MMTKFlags => MemorySize, min_heap, MemorySize::from_str("64M").unwrap(), "Minimum heap size (default 64MB)");
define_flag!(MMTKFlags => MemorySize, max_heap, MemorySize::from_str("256M").unwrap(), "Maximum heap size (default 256MB)");
define_option_handler!(MMTKFlags => parse_gc_trigger, trigger, "Select GC trigger (default: Dynamic)");
define_flag!(MMTKFlags => 
    f64, 
    min_nursery, 
    0.25, 
    "The lower bound of the nursery size as a proportion of the current heap size. (default 0.25)");
define_flag!(MMTKFlags =>
    f64,
    max_nursery,
    1.0,
    "The upper bound of the nursery size as a proportion of the current heap size. (default 1.0)"
);

define_flag!(MMTKFlags =>
    MemorySize,
    fixed_nursery,
    MemorySize::from_str("4M").unwrap(),
    "Fixed nursery size"
);

define_flag!(MMTKFlags =>
    MemorySize,
    min_nursery_bound,
    MemorySize::from_str("4M").unwrap(),
    "The lower bound of the nursery size in bytes. (default 4M)"
);

define_flag!(MMTKFlags =>
    MemorySize,
    max_nursery_bound,
    MemorySize::from_str("128M").unwrap(),
    "The upper bound of the nursery size in bytes. (default 128M)"
);
define_flag!(MMTKFlags =>
    bool,
    ignore_system_gc,
    false,
    "Ignore GC requested by the user. (default: false)"
);

define_flag!(MMTKFlags =>
    bool,
    full_heap_system_gc,
    false,
    "Force major GC on a system GC. (default: false)"
);

define_flag!(MMTKFlags =>
    usize,
    threads,
    std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1),
    "Number of GC worker threads. (default: number of cores)"
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectedGCPlan {
    None,
    Immix,
    StickyImmix,
    GenImmix,
    GenCopy,
    MarkSweep,
    SemiSpace,
    NotSelected,
    NoGC,
}
unsafe impl bytemuck::NoUninit for SelectedGCPlan {}

static PLAN: Atomic<SelectedGCPlan> = Atomic::new(SelectedGCPlan::NotSelected);

fn parse_gc_plan(option: &str) {
    let plan = match option.to_lowercase().as_str() {
        "none" => SelectedGCPlan::None,
        "immix" => SelectedGCPlan::Immix,
        "stickyimmix" => SelectedGCPlan::StickyImmix,
        "genimmix" => SelectedGCPlan::GenImmix,
        "gencopy" => SelectedGCPlan::GenCopy,
        "marksweep" => SelectedGCPlan::MarkSweep,
        "semispace" => SelectedGCPlan::SemiSpace,
        "nogc" => SelectedGCPlan::NoGC,
        _ => SelectedGCPlan::NotSelected,
    };

    PLAN.store(plan, Ordering::Relaxed);
}



#[derive(Clone, PartialEq, Eq, Debug)]
enum SelectedGCTrigger {
    Dynamic,
    Fixed,
    Custom(String),
}

static TRIGGER: Mutex<SelectedGCTrigger> = Mutex::new(SelectedGCTrigger::Dynamic);
fn parse_gc_trigger(option: &str) {
    let trigger = match option.to_lowercase().as_str() {
        "dynamic" => SelectedGCTrigger::Dynamic,
        "fixed" => SelectedGCTrigger::Fixed,
        _ => SelectedGCTrigger::Custom(option.to_owned()),
    };

    *TRIGGER.lock() = trigger;
}

static CURRENT_PLAN: OnceLock<PlanSelector> = OnceLock::new();

pub fn vmkit_current_plan() -> PlanSelector {
    CURRENT_PLAN.get().copied().unwrap()
}

pub(super) fn mmtk_options(builder: &mut MMTKBuilder) -> Result<(), String> {
    let max_heap = *mmtkflags_max_heap();
    let min_heap = *mmtkflags_min_heap();

    if max_heap.0 < min_heap.0 {
        return Err(format!(
            "max heap size is smaller than min heap size: {} < {}",
            max_heap, min_heap
        ));
    }

    builder
        .options
        .gc_trigger
        .set(match TRIGGER.lock().clone() {
            SelectedGCTrigger::Custom(_) => unimplemented!("not supported"),
            SelectedGCTrigger::Fixed => GCTriggerSelector::FixedHeapSize(max_heap.0),
            SelectedGCTrigger::Dynamic => {
                GCTriggerSelector::DynamicHeapSize(min_heap.0, max_heap.0)
            }
        });
    let sel = match PLAN.load(Ordering::Relaxed) {
            SelectedGCPlan::None => PlanSelector::NoGC,
            SelectedGCPlan::GenCopy => PlanSelector::GenCopy,
            SelectedGCPlan::GenImmix => PlanSelector::GenImmix,
            SelectedGCPlan::NotSelected => PlanSelector::GenImmix,
            SelectedGCPlan::MarkSweep => PlanSelector::MarkSweep,
            SelectedGCPlan::StickyImmix => PlanSelector::StickyImmix,
            SelectedGCPlan::SemiSpace => PlanSelector::SemiSpace,
            SelectedGCPlan::Immix => PlanSelector::Immix,
            SelectedGCPlan::NoGC => PlanSelector::NoGC,
    };
    builder
        .options
        .plan
        .set(sel);

    let nursery_size =
        if is_mmtkflags_max_nursery_bound_set() || is_mmtkflags_min_nursery_bound_set() {
            NurserySize::Bounded {
                min: mmtkflags_min_nursery_bound().0,
                max: mmtkflags_max_nursery_bound().0,
            }
        } else if is_mmtkflags_fixed_nursery_set() {
            NurserySize::Fixed(mmtkflags_fixed_nursery().0)
        } else {
            NurserySize::ProportionalBounded {
                min: *mmtkflags_min_nursery(),
                max: *mmtkflags_max_nursery(),
            }
        };

    builder.options.nursery.set(nursery_size);
    builder
        .options
        .ignore_system_gc
        .set(*mmtkflags_ignore_system_gc());
    builder
        .options
        .full_heap_system_gc
        .set(*mmtkflags_full_heap_system_gc());

    let threads = *mmtkflags_threads();

    if threads == 0 {
        return Err(format!("number of GC threads cannot be zero"));
    }

    builder.options.threads.set(threads);

    Ok(())
}
