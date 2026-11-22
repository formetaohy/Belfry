use belfry_compute_abi::{
    Kind, PlacementRecord, SegmentRecord, StepRecord, TaskRecord, ValueRecord,
};
use bytemuck::Zeroable;
use std::mem::{align_of, offset_of, size_of};

fn refuses(action: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    std::panic::catch_unwind(action).is_err()
}

#[test]
fn records_follow_the_shader_layout() {
    assert_eq!(size_of::<ValueRecord>(), 80);
    assert_eq!(offset_of!(ValueRecord, base), 0);
    assert_eq!(offset_of!(ValueRecord, store), 4);
    assert_eq!(offset_of!(ValueRecord, table), 12);
    assert_eq!(offset_of!(ValueRecord, storage), 16);
    assert_eq!(offset_of!(ValueRecord, free), 20);
    assert_eq!(offset_of!(ValueRecord, rank), 24);
    assert_eq!(offset_of!(ValueRecord, bounds), 32);
    assert_eq!(offset_of!(ValueRecord, dims), 48);
    assert_eq!(offset_of!(ValueRecord, strides), 64);
    assert_eq!(size_of::<TaskRecord>(), 208);
    assert_eq!(offset_of!(TaskRecord, op), 4);
    assert_eq!(offset_of!(TaskRecord, geometry), 8);
    assert_eq!(offset_of!(TaskRecord, count), 16);
    assert_eq!(offset_of!(TaskRecord, splits), 24);
    assert_eq!(offset_of!(TaskRecord, out), 28);
    assert_eq!(offset_of!(TaskRecord, extra), 32);
    assert_eq!(offset_of!(TaskRecord, a), 36);
    assert_eq!(offset_of!(TaskRecord, b), 40);
    assert_eq!(offset_of!(TaskRecord, c), 44);
    assert_eq!(offset_of!(TaskRecord, d), 48);
    assert_eq!(offset_of!(TaskRecord, e), 52);
    assert_eq!(offset_of!(TaskRecord, f), 56);
    assert_eq!(offset_of!(TaskRecord, origin), 60);
    assert_eq!(offset_of!(TaskRecord, literal), 64);
    assert_eq!(offset_of!(TaskRecord, prelude), 68);
    assert_eq!(offset_of!(TaskRecord, prelude_steps), 72);
    assert_eq!(offset_of!(TaskRecord, chain), 76);
    assert_eq!(offset_of!(TaskRecord, steps), 80);
    assert_eq!(offset_of!(TaskRecord, reach_rows), 84);
    assert_eq!(offset_of!(TaskRecord, reach_columns), 88);
    assert_eq!(offset_of!(TaskRecord, stride_rows), 92);
    assert_eq!(offset_of!(TaskRecord, stride_columns), 96);
    assert_eq!(offset_of!(TaskRecord, pad_rows), 100);
    assert_eq!(offset_of!(TaskRecord, pad_columns), 104);
    assert_eq!(offset_of!(TaskRecord, dilation_rows), 108);
    assert_eq!(offset_of!(TaskRecord, dilation_columns), 112);
    assert_eq!(offset_of!(TaskRecord, axis), 116);
    assert_eq!(offset_of!(TaskRecord, offset), 120);
    assert_eq!(offset_of!(TaskRecord, in_place), 124);
    assert_eq!(offset_of!(TaskRecord, wave), 128);
    assert_eq!(offset_of!(TaskRecord, segment), 160);
    assert_eq!(offset_of!(TaskRecord, keys), 164);
    assert_eq!(offset_of!(TaskRecord, reach), 168);
    assert_eq!(offset_of!(TaskRecord, queries), 172);
    assert_eq!(offset_of!(TaskRecord, tokens), 176);
    assert_eq!(offset_of!(TaskRecord, grid), 180);
    assert_eq!(offset_of!(TaskRecord, planned_first), 184);
    assert_eq!(offset_of!(TaskRecord, planned_count), 188);
    assert_eq!(offset_of!(TaskRecord, knob), 192);
    assert_eq!(offset_of!(TaskRecord, keep), 196);
    assert_eq!(offset_of!(TaskRecord, section), 200);
    assert_eq!(offset_of!(TaskRecord, bias), 204);
    assert_eq!(size_of::<StepRecord>(), 12);
    assert_eq!(offset_of!(StepRecord, op), 0);
    assert_eq!(offset_of!(StepRecord, operand), 4);
    assert_eq!(offset_of!(StepRecord, swapped), 8);
    assert_eq!(size_of::<SegmentRecord>(), 24);
    assert_eq!(offset_of!(SegmentRecord, first), 0);
    assert_eq!(offset_of!(SegmentRecord, count), 4);
    assert_eq!(offset_of!(SegmentRecord, wave), 8);
    assert_eq!(offset_of!(SegmentRecord, predecessors), 12);
    assert_eq!(offset_of!(SegmentRecord, successors), 16);
    assert_eq!(offset_of!(SegmentRecord, successors_count), 20);
    assert_eq!(align_of::<ValueRecord>(), 4);
    assert_eq!(align_of::<TaskRecord>(), 4);
    assert_eq!(align_of::<StepRecord>(), 4);
    assert_eq!(size_of::<PlacementRecord>(), 8);
    assert_eq!(offset_of!(PlacementRecord, tensors), 0);
    assert_eq!(offset_of!(PlacementRecord, weights), 4);
}

#[test]
fn every_task_kind_declares_the_device_code_it_runs() {
    for kind in Kind::ALL {
        let info = kind.info();
        assert_eq!(info.kind, *kind);
        assert_eq!(kind.name(), info.name);
        assert!(
            info.entry.starts_with("run_"),
            "the {} kind names its device body {}",
            kind.name(),
            info.entry,
        );
        let mut modules = info.modules.to_vec();
        let declared = modules.len();
        modules.sort_unstable_by_key(|module| module.name());
        modules.dedup();
        assert_eq!(
            modules.len(),
            declared,
            "the {} kind installs the same device module twice",
            kind.name(),
        );
    }
}

#[test]
fn a_zeroed_record_holds_no_number_the_shader_could_read() {
    let placement = PlacementRecord::zeroed();
    let segment = SegmentRecord::zeroed();
    let step = StepRecord::zeroed();
    let task = TaskRecord::zeroed();
    let value = ValueRecord::zeroed();
    let records: [&[u8]; 5] = [
        bytemuck::bytes_of(&placement),
        bytemuck::bytes_of(&segment),
        bytemuck::bytes_of(&step),
        bytemuck::bytes_of(&task),
        bytemuck::bytes_of(&value),
    ];
    for bytes in records {
        assert!(
            bytes.iter().all(|byte| *byte == 0),
            "a zeroed record carries {bytes:?} where the shader reads every byte of it",
        );
    }
}

#[test]
fn a_refusal_carries_the_subject_it_names_and_the_reason_it_refused() {
    use belfry_compute_abi::refusal::{CODE_LIMIT, KIND_BITS};
    use belfry_compute_abi::{Refusal, TENSOR};
    assert_eq!(TENSOR, Kind::COUNT);
    for reason in Refusal::ALL {
        for subject in [0u32, Kind::Attention.code(), TENSOR] {
            assert_eq!(
                Refusal::read(reason.word(subject, 9)),
                (subject, *reason, 9),
                "the {} refusal leaves the subject it names",
                reason.name(),
            );
        }
        assert!(refuses(|| {
            let _ = reason.word(Kind::Attention.code(), CODE_LIMIT);
        }));
        assert!(refuses(|| {
            let _ = reason.word(1 << KIND_BITS, 0);
        }));
    }
    let mut names = Refusal::ALL
        .iter()
        .map(|reason| reason.name())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), Refusal::ALL.len(), "two reasons share a name");
    for (code, reason) in Refusal::ALL.iter().enumerate() {
        assert_eq!(reason.code(), code as u32, "the reasons leave a hole");
        assert_eq!(Refusal::of(code as u32), *reason);
    }
    assert!(refuses(|| {
        let _ = Refusal::of(Refusal::COUNT);
    }));
    assert!(refuses(|| {
        let _ = Refusal::read(0);
    }));
}
