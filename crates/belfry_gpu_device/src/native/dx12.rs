use super::{
    DeviceFailure, FRAME_TIMEOUT, FRAMES_IN_FLIGHT, NativeBuffer, NativeFrame,
    NativeGraphicsPipeline, NativeGroup, NativePipeline, NativePresent, NativeTexture,
    STAGING_BYTES, TIME_SLOTS, hang_deadline,
};
use crate::backend::Backend;
use crate::buffer::GpuBuffer;
use crate::cache::{ArtifactCache, fingerprint};
use crate::capability::{
    AdapterId, AdapterInfo, AdapterPolicy, BufferUsages, Capability, DeviceType, Limits,
    PowerPreference, TextureUsages,
};
use crate::gate::Gate;
use crate::graphics::{BlendState, DepthState, PipelineState};
use crate::host::{HOST_SLICE, HostAdvance};
use crate::pipeline::BoundResource;
use crate::sampler::{AddressMode, Filter, SamplerDesc};
use crate::submission::{Command, RecordedCount, Write};
use crate::surface::WindowHandle;
use crate::texture::TextureFormat;
use belfry_gpu_shader::{
    BindingSpec, ComputeProgram, GraphicsProgram, GraphicsTranslation, MAX_BINDING_BYTES,
    ShaderTranslation,
};
use libloading::Library;
use std::any::Any;
use std::ffi::c_void;
use std::mem::{ManuallyDrop, size_of};
use std::ptr;
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT, WAIT_OBJECT_0};
use windows::Win32::Graphics::Direct3D::Dxc::*;
use windows::Win32::Graphics::Direct3D::{
    D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, ID3DBlob,
};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_D32_FLOAT,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R16G16B16A16_FLOAT,
    DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_SOFTWARE,
    DXGI_ERROR_NOT_FOUND, DXGI_GPU_PREFERENCE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
    DXGI_GPU_PREFERENCE_MINIMUM_POWER, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    IDXGIAdapter1, IDXGIFactory1, IDXGIFactory2, IDXGIFactory4, IDXGIFactory6, IDXGISwapChain3,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{BOOL, GUID, Interface, PCWSTR};

const RELEASE_TIMEOUT: Duration = Duration::from_secs(30);
const SWAPCHAIN_BUFFERS: u32 = 3;
const DXC_FLAGS: &[&str] = &["-O3", "-Wno-parentheses-equality", "-WX"];
const DEPTH_FORMAT: DXGI_FORMAT = DXGI_FORMAT_D32_FLOAT;

struct CompletionEvent(HANDLE);

unsafe impl Send for CompletionEvent {}
unsafe impl Sync for CompletionEvent {}

impl Drop for CompletionEvent {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) }.expect("a compute completion event closes");
    }
}

struct Frame {
    index: u64,
    first_dispatch: bool,
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    staging: Arc<BufferResource>,
    cursor: u64,
    resources: Vec<Arc<dyn Any + Send + Sync>>,
}

impl Frame {
    fn new(device: &ID3D12Device) -> Self {
        let allocator = unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
            .unwrap_or_else(|error| panic!("allocating a D3D12 command allocator: {error}"));
        let list: ID3D12GraphicsCommandList = unsafe {
            device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .unwrap_or_else(|error| panic!("creating a D3D12 compute command list: {error}"));
        unsafe { list.Close() }
            .unwrap_or_else(|error| panic!("closing a D3D12 compute command list: {error}"));
        Self {
            index: 0,
            first_dispatch: false,
            allocator,
            list,
            staging: allocate(device, STAGING_BYTES, D3D12_HEAP_TYPE_UPLOAD, false),
            cursor: 0,
            resources: Vec::new(),
        }
    }

    fn begin(&mut self) {
        unsafe { self.allocator.Reset() }
            .unwrap_or_else(|error| panic!("recycling a D3D12 compute command allocator: {error}"));
        unsafe { self.list.Reset(&self.allocator, None) }
            .unwrap_or_else(|error| panic!("recycling a D3D12 compute command list: {error}"));
        self.cursor = 0;
        self.resources.clear();
    }

    fn stage(&mut self, device: &ID3D12Device, bytes: &[u8]) -> (ID3D12Resource, u64) {
        let size = bytes.len() as u64;
        let cursor = self.cursor.next_multiple_of(512);
        let (upload, offset) = if size <= STAGING_BYTES.saturating_sub(cursor) {
            self.cursor = cursor + size;
            (self.staging.clone(), cursor)
        } else {
            let upload = allocate(device, size, D3D12_HEAP_TYPE_UPLOAD, false);
            self.resources.push(upload.clone());
            (upload, 0)
        };
        let mut mapped = ptr::null_mut();
        unsafe {
            upload.raw.Map(
                0,
                Some(&D3D12_RANGE { Begin: 0, End: 0 }),
                Some(&raw mut mapped),
            )
        }
        .unwrap_or_else(|error| panic!("mapping a D3D12 upload: {error}"));
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                mapped.cast::<u8>().add(offset as usize),
                bytes.len(),
            );
            upload.raw.Unmap(
                0,
                Some(&D3D12_RANGE {
                    Begin: offset as usize,
                    End: (offset + size) as usize,
                }),
            );
        }
        (upload.raw.clone(), offset)
    }
}

struct QueueState {
    next: u64,
    completed: u64,
    frames: Vec<Frame>,
    times: [Option<(u64, f64)>; TIME_SLOTS],
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            next: 0,
            completed: 0,
            frames: Vec::new(),
            times: [None; TIME_SLOTS],
        }
    }
}

struct DescriptorHeap {
    raw: ID3D12DescriptorHeap,
    cpu: D3D12_CPU_DESCRIPTOR_HANDLE,
    gpu: D3D12_GPU_DESCRIPTOR_HANDLE,
    slot_bytes: u32,
    free: Mutex<Vec<u32>>,
}

impl DescriptorHeap {
    fn new(
        device: &ID3D12Device,
        heap_type: D3D12_DESCRIPTOR_HEAP_TYPE,
        capacity: u32,
        label: &str,
    ) -> Self {
        let raw: ID3D12DescriptorHeap = unsafe {
            device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: heap_type,
                NumDescriptors: capacity,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 1,
            })
        }
        .unwrap_or_else(|error| panic!("creating the D3D12 {label} heap: {error}"));
        Self {
            cpu: unsafe { raw.GetCPUDescriptorHandleForHeapStart() },
            gpu: unsafe { raw.GetGPUDescriptorHandleForHeapStart() },
            slot_bytes: unsafe { device.GetDescriptorHandleIncrementSize(heap_type) },
            free: Mutex::new((0..capacity).rev().collect()),
            raw,
        }
    }

    fn allocate(&self, label: &str) -> u32 {
        let slot = self
            .free
            .lock()
            .expect("a D3D12 descriptor heap is never poisoned")
            .pop();
        match slot {
            Some(slot) => slot,
            None => panic!("the D3D12 {label} heap holds no free descriptor"),
        }
    }

    fn release(&self, slot: u32) {
        self.free
            .lock()
            .expect("a D3D12 descriptor heap is never poisoned")
            .push(slot);
    }

    fn allocate_range(&self, count: u32, label: &str) -> u32 {
        let mut free = self
            .free
            .lock()
            .expect("a D3D12 descriptor heap is never poisoned");
        free.sort_unstable();
        let start = free
            .windows(count as usize)
            .find(|window| {
                window
                    .windows(2)
                    .all(|pair| pair[1] == pair[0].saturating_add(1))
            })
            .map(|window| window[0]);
        let Some(start) = start else {
            drop(free);
            panic!("the D3D12 {label} heap holds no {count} free descriptors");
        };
        let at = free
            .iter()
            .position(|slot| *slot == start)
            .expect("a free descriptor run starts at a free slot");
        free.drain(at..at + count as usize);
        start
    }

    fn release_range(&self, start: u32, count: u32) {
        self.free
            .lock()
            .expect("a D3D12 descriptor heap is never poisoned")
            .extend(start..start + count);
    }

    fn cpu(&self, slot: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: self.cpu.ptr + slot as usize * self.slot_bytes as usize,
        }
    }

    fn gpu(&self, slot: u32) -> D3D12_GPU_DESCRIPTOR_HANDLE {
        D3D12_GPU_DESCRIPTOR_HANDLE {
            ptr: self.gpu.ptr + slot as u64 * u64::from(self.slot_bytes),
        }
    }
}

pub(crate) struct TextureTable {
    raw: ID3D12Device,
    heap: Arc<DescriptorHeap>,
    start: u32,
    count: u32,
}

impl TextureTable {
    fn fill(&self, index: u32, texture: &crate::Texture) {
        let texture = native_texture(texture.native());
        unsafe {
            self.raw.CreateShaderResourceView(
                &texture.raw,
                Some(&srv_desc(
                    texture.desc.Format,
                    u32::from(texture.desc.MipLevels),
                )),
                self.heap.cpu(self.start + index),
            );
        }
    }

    pub(crate) fn gpu(&self) -> D3D12_GPU_DESCRIPTOR_HANDLE {
        self.heap.gpu(self.start)
    }
}

impl Drop for TextureTable {
    fn drop(&mut self) {
        self.heap.release_range(self.start, self.count);
    }
}

pub(crate) struct SamplerTable {
    raw: ID3D12Device,
    heap: Arc<DescriptorHeap>,
    start: u32,
    count: u32,
}

impl SamplerTable {
    fn fill(&self, index: u32, sampler: &crate::Sampler) {
        let sampler = native_sampler(sampler.native());
        unsafe {
            self.raw.CreateSampler(
                &sampler_description(&sampler.desc, sampler.anisotropy),
                self.heap.cpu(self.start + index),
            );
        }
    }

    pub(crate) fn gpu(&self) -> D3D12_GPU_DESCRIPTOR_HANDLE {
        self.heap.gpu(self.start)
    }
}

impl Drop for SamplerTable {
    fn drop(&mut self) {
        self.heap.release_range(self.start, self.count);
    }
}

pub(crate) struct Sampler {
    heap: Arc<DescriptorHeap>,
    slot: u32,
    desc: SamplerDesc,
    anisotropy: u32,
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.heap.release(self.slot);
    }
}

pub(crate) struct Device {
    raw: ID3D12Device,
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    event: CompletionEvent,
    zero: Arc<BufferResource>,
    stamps: ID3D12QueryHeap,
    clock: Arc<BufferResource>,
    srv: Arc<DescriptorHeap>,
    samplers: Arc<DescriptorHeap>,
    frequency: u64,
    state: Mutex<QueueState>,
    pub(crate) gate: Arc<Gate>,
    window: Option<usize>,
    _keep_alive: Option<Arc<dyn Any + Send + Sync>>,
}

struct Lent {
    device: usize,
    native: Weak<Device>,
    info: AdapterInfo,
    limits: Limits,
}

static LENT: LazyLock<Mutex<Vec<Lent>>> = LazyLock::new(|| Mutex::new(Vec::new()));

struct BufferResource {
    raw: ID3D12Resource,
    state: Mutex<D3D12_RESOURCE_STATES>,
}

#[derive(Clone)]
pub(crate) struct Buffer {
    resource: Arc<BufferResource>,
}

struct PipelineResource {
    root: ID3D12RootSignature,
    compiled: OnceLock<ID3D12PipelineState>,
}

#[derive(Clone)]
pub(crate) struct Texture {
    resource: Arc<TextureResource>,
}

pub(crate) struct Surface {
    device: Arc<Device>,
    raw: IDXGISwapChain3,
    buffers: Vec<Texture>,
    width: u32,
    height: u32,
}

#[derive(Clone)]
pub(crate) struct Present {
    raw: IDXGISwapChain3,
}

struct Target {
    _heap: ID3D12DescriptorHeap,
    view: D3D12_CPU_DESCRIPTOR_HANDLE,
    depth: ID3D12Resource,
    depth_state: Mutex<D3D12_RESOURCE_STATES>,
    _depth_heap: ID3D12DescriptorHeap,
    depth_view: D3D12_CPU_DESCRIPTOR_HANDLE,
}

struct TextureResource {
    raw: ID3D12Resource,
    state: Mutex<D3D12_RESOURCE_STATES>,
    target: Option<Target>,
    srv: Option<(Arc<DescriptorHeap>, u32)>,
    desc: D3D12_RESOURCE_DESC,
    width: u32,
    height: u32,
    format: TextureFormat,
    usages: TextureUsages,
}

impl Drop for TextureResource {
    fn drop(&mut self) {
        if let Some((heap, slot)) = &self.srv {
            heap.release(*slot);
        }
    }
}

pub(crate) struct GraphicsPipeline {
    resource: Arc<GraphicsPipelineResource>,
}

struct GraphicsPipelineResource {
    root: ID3D12RootSignature,
    format: TextureFormat,
    state: PipelineState,
    compiled: OnceLock<ID3D12PipelineState>,
    signature: OnceLock<ID3D12CommandSignature>,
}

pub(crate) struct Pipeline {
    resource: Arc<PipelineResource>,
}

fn native_buffer(native: &NativeBuffer) -> &Arc<BufferResource> {
    let NativeBuffer::Dx12(buffer) = native else {
        panic!("a D3D12 command cannot use another backend's buffer");
    };
    &buffer.resource
}

fn buffer(gpu: &GpuBuffer) -> &Arc<BufferResource> {
    native_buffer(gpu.native())
}

fn texture_table(native: &crate::native::NativeTextureTable) -> &TextureTable {
    let crate::native::NativeTextureTable::Dx12(table) = native else {
        panic!("a D3D12 command cannot use another backend's texture table");
    };
    table
}

fn sampler_table(native: &crate::native::NativeSamplerTable) -> &SamplerTable {
    let crate::native::NativeSamplerTable::Dx12(table) = native else {
        panic!("a D3D12 command cannot use another backend's sampler table");
    };
    table
}

fn native_sampler(native: &crate::NativeSampler) -> &Sampler {
    let crate::NativeSampler::Dx12(sampler) = native else {
        panic!("a D3D12 command cannot use another backend's sampler");
    };
    sampler
}

fn native_texture(native: &NativeTexture) -> &Arc<TextureResource> {
    let NativeTexture::Dx12(texture) = native else {
        panic!("a D3D12 command cannot use another backend's texture");
    };
    &texture.resource
}

fn graphics_pipeline(native: &NativeGraphicsPipeline) -> &Arc<GraphicsPipelineResource> {
    let NativeGraphicsPipeline::Dx12(pipeline) = native else {
        panic!("a D3D12 command cannot use another backend's graphics pipeline");
    };
    &pipeline.resource
}

fn pipeline(native: &NativePipeline) -> &Arc<PipelineResource> {
    let NativePipeline::Dx12(pipeline) = native else {
        panic!("a D3D12 command cannot use another backend's pipeline");
    };
    &pipeline.resource
}

fn claim_group(native: &NativeGroup) {
    match native {
        NativeGroup::Dx12 => {}
        #[cfg(multiple_backends)]
        _ => panic!("a D3D12 command cannot use another backend's bind group"),
    }
}

fn srv_desc(format: DXGI_FORMAT, mip_levels: u32) -> D3D12_SHADER_RESOURCE_VIEW_DESC {
    D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: format,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV {
                MostDetailedMip: 0,
                MipLevels: mip_levels,
                PlaneSlice: 0,
                ResourceMinLODClamp: 0.0,
            },
        },
    }
}

fn sampler_description(desc: &SamplerDesc, anisotropy: u32) -> D3D12_SAMPLER_DESC {
    let address = |mode: AddressMode| match mode {
        AddressMode::ClampToEdge => D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
        AddressMode::Repeat => D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        AddressMode::MirrorRepeat => D3D12_TEXTURE_ADDRESS_MODE_MIRROR,
    };
    let filter = if anisotropy > 1 {
        D3D12_FILTER_ANISOTROPIC
    } else {
        match (desc.min_filter, desc.mag_filter, desc.mip_filter) {
            (Filter::Nearest, Filter::Nearest, Filter::Nearest) => D3D12_FILTER_MIN_MAG_MIP_POINT,
            (Filter::Nearest, Filter::Nearest, Filter::Linear) => {
                D3D12_FILTER_MIN_MAG_POINT_MIP_LINEAR
            }
            (Filter::Nearest, Filter::Linear, Filter::Nearest) => {
                D3D12_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT
            }
            (Filter::Nearest, Filter::Linear, Filter::Linear) => {
                D3D12_FILTER_MIN_POINT_MAG_MIP_LINEAR
            }
            (Filter::Linear, Filter::Nearest, Filter::Nearest) => {
                D3D12_FILTER_MIN_LINEAR_MAG_MIP_POINT
            }
            (Filter::Linear, Filter::Nearest, Filter::Linear) => {
                D3D12_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR
            }
            (Filter::Linear, Filter::Linear, Filter::Nearest) => {
                D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT
            }
            (Filter::Linear, Filter::Linear, Filter::Linear) => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
        }
    };
    D3D12_SAMPLER_DESC {
        Filter: filter,
        AddressU: address(desc.address_u),
        AddressV: address(desc.address_v),
        AddressW: address(desc.address_u),
        MipLODBias: 0.0,
        MaxAnisotropy: anisotropy,
        ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
        BorderColor: [0.0; 4],
        MinLOD: 0.0,
        MaxLOD: f32::MAX,
    }
}

fn render_target_blend(state: PipelineState) -> D3D12_RENDER_TARGET_BLEND_DESC {
    let (source, destination, source_alpha, destination_alpha) = match state.blend {
        BlendState::Opaque => (
            D3D12_BLEND_ONE,
            D3D12_BLEND_ZERO,
            D3D12_BLEND_ONE,
            D3D12_BLEND_ZERO,
        ),
        BlendState::Premultiplied => (
            D3D12_BLEND_ONE,
            D3D12_BLEND_INV_SRC_ALPHA,
            D3D12_BLEND_INV_DEST_ALPHA,
            D3D12_BLEND_ONE,
        ),
        BlendState::Preserve => (
            D3D12_BLEND_ZERO,
            D3D12_BLEND_ONE,
            D3D12_BLEND_ZERO,
            D3D12_BLEND_ONE,
        ),
    };
    D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: (!matches!(state.blend, BlendState::Opaque)).into(),
        LogicOpEnable: false.into(),
        SrcBlend: source,
        DestBlend: destination,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: source_alpha,
        DestBlendAlpha: destination_alpha,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        RenderTargetWriteMask: if state.alpha {
            D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8
        } else {
            (D3D12_COLOR_WRITE_ENABLE_RED.0
                | D3D12_COLOR_WRITE_ENABLE_GREEN.0
                | D3D12_COLOR_WRITE_ENABLE_BLUE.0) as u8
        },
    }
}

fn dxgi_format(format: TextureFormat) -> DXGI_FORMAT {
    match format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM,
        TextureFormat::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
        TextureFormat::Rgba16Float => DXGI_FORMAT_R16G16B16A16_FLOAT,
        TextureFormat::R32Float => DXGI_FORMAT_R32_FLOAT,
    }
}

fn dxgi_view(format: TextureFormat) -> DXGI_FORMAT {
    match format {
        TextureFormat::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        held => dxgi_format(held),
    }
}

fn software(desc: &DXGI_ADAPTER_DESC1) -> bool {
    DXGI_ADAPTER_FLAG(desc.Flags as i32).contains(DXGI_ADAPTER_FLAG_SOFTWARE)
}

fn architecture(device: &ID3D12Device) -> DeviceType {
    let mut data = D3D12_FEATURE_DATA_ARCHITECTURE {
        NodeIndex: 0,
        ..Default::default()
    };
    let queried = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_ARCHITECTURE,
            (&raw mut data).cast(),
            size_of::<D3D12_FEATURE_DATA_ARCHITECTURE>() as u32,
        )
    }
    .is_ok();
    if queried && data.UMA.as_bool() {
        DeviceType::Integrated
    } else {
        DeviceType::Discrete
    }
}

fn preference(policy: AdapterPolicy) -> DXGI_GPU_PREFERENCE {
    match policy {
        AdapterPolicy::Power(PowerPreference::LowPower) => DXGI_GPU_PREFERENCE_MINIMUM_POWER,
        AdapterPolicy::Power(PowerPreference::HighPerformance) | AdapterPolicy::Identity(_) => {
            DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE
        }
    }
}

fn adapters(factory: &IDXGIFactory1, policy: AdapterPolicy) -> Result<Vec<IDXGIAdapter1>, String> {
    let preferred = factory.cast::<IDXGIFactory6>().ok();
    let gpu = preference(policy);
    let mut adapters = Vec::new();
    for index in 0.. {
        let adapter = match &preferred {
            Some(factory) => unsafe { factory.EnumAdapterByGpuPreference(index, gpu) },
            None => unsafe { factory.EnumAdapters1(index) },
        };
        match adapter {
            Ok(adapter) => adapters.push(adapter),
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(format!("enumerating DXGI adapters: {error}")),
        }
    }
    Ok(adapters)
}

fn describe(desc: &DXGI_ADAPTER_DESC1) -> AdapterInfo {
    AdapterInfo {
        name: String::from_utf16_lossy(
            &desc.Description[..desc
                .Description
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(desc.Description.len())],
        ),
        backend: Backend::Dx12,
        device_type: if software(desc) {
            DeviceType::Cpu
        } else {
            DeviceType::Other
        },
        id: AdapterId::Numeric {
            vendor: desc.VendorId,
            device: desc.DeviceId,
        },
    }
}

fn describe_device(device: &ID3D12Device) -> Result<AdapterInfo, String> {
    let luid = unsafe { device.GetAdapterLuid() };
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|error| format!("creating DXGI: {error}"))?;
    let factory: IDXGIFactory4 = factory
        .cast()
        .map_err(|error| format!("asking DXGI for its fourth factory: {error}"))?;
    let adapter: IDXGIAdapter1 = unsafe { factory.EnumAdapterByLuid(luid) }
        .map_err(|error| format!("finding the adapter of a D3D12 device: {error}"))?;
    let desc = unsafe { adapter.GetDesc1() }
        .map_err(|error| format!("querying the adapter of a D3D12 device: {error}"))?;
    let mut info = describe(&desc);
    info.device_type = if software(&desc) {
        DeviceType::Cpu
    } else {
        architecture(device)
    };
    Ok(info)
}

fn limits(device: &ID3D12Device) -> Limits {
    let mut space = D3D12_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT::default();
    unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT,
            (&raw mut space).cast(),
            size_of::<D3D12_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT>() as u32,
        )
    }
    .unwrap_or_else(|error| {
        panic!("querying the GPU virtual address space of a D3D12 device: {error}")
    });
    let bits = space.MaxGPUVirtualAddressBitsPerResource;
    assert!(
        bits >= 26,
        "a D3D12 device addresses {bits} bits of one resource, and the {} byte buffer of the baseline does not fit",
        Limits::BASELINE.max_buffer_size,
    );
    let bytes = MAX_BINDING_BYTES.min(1u64 << bits.min(63));
    Limits {
        max_storage_buffers_per_shader_stage: 30,
        max_sampled_images_per_shader_stage: u32::MAX,
        max_samplers_per_shader_stage: u32::MAX,
        max_storage_buffer_binding_size: bytes,
        max_buffer_size: bytes,
        max_compute_invocations_per_workgroup: 1024,
        max_compute_workgroup_size_x: 1024,
        max_compute_workgroup_storage_size: 32 << 10,
        max_compute_workgroups_per_dimension: 65_535,
        min_storage_buffer_offset_alignment: 16,
        max_sampler_anisotropy: 16,
    }
}

fn compiler() -> Result<&'static Library, String> {
    static COMPILER: OnceLock<Result<Library, String>> = OnceLock::new();
    COMPILER
        .get_or_init(|| {
            unsafe { Library::new("dxcompiler.dll") }.map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| error.clone())
}

struct Candidate {
    device: ID3D12Device,
    info: AdapterInfo,
    limits: Limits,
}

fn shader_model(device: &ID3D12Device) -> bool {
    let mut model = D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: D3D_SHADER_MODEL_6_0,
    };
    let queried = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_SHADER_MODEL,
            (&raw mut model).cast(),
            size_of::<D3D12_FEATURE_DATA_SHADER_MODEL>() as u32,
        )
    }
    .is_ok();
    queried && model.HighestShaderModel.0 >= D3D_SHADER_MODEL_6_0.0
}

fn candidate(
    adapter: &IDXGIAdapter1,
    desc: &DXGI_ADAPTER_DESC1,
) -> Result<Option<Candidate>, String> {
    if cfg!(debug_assertions) {
        let mut debug = None;
        if unsafe { D3D12GetDebugInterface::<ID3D12Debug>(&mut debug) }.is_ok()
            && let Some(debug) = debug
        {
            unsafe { debug.EnableDebugLayer() };
        }
    }
    let mut raw = None;
    if unsafe { D3D12CreateDevice(adapter, D3D_FEATURE_LEVEL_11_0, &mut raw) }.is_err() {
        return Ok(None);
    }
    let raw: ID3D12Device = raw.expect("a successfully created D3D12 device exists");
    if !shader_model(&raw) {
        return Ok(None);
    }
    let mut info = describe(desc);
    info.device_type = if software(desc) {
        DeviceType::Cpu
    } else {
        architecture(&raw)
    };
    let limits = limits(&raw);
    Ok(Some(Candidate {
        device: raw,
        info,
        limits,
    }))
}

fn by_identity(
    adapters: &[(IDXGIAdapter1, DXGI_ADAPTER_DESC1)],
    wanted: AdapterId,
) -> Result<Option<Candidate>, String> {
    for (adapter, desc) in adapters {
        if describe(desc).id != wanted {
            continue;
        }
        return candidate(adapter, desc);
    }
    Ok(None)
}

fn by_power(
    adapters: &[(IDXGIAdapter1, DXGI_ADAPTER_DESC1)],
    preference: PowerPreference,
) -> Result<Option<Candidate>, String> {
    let preferred = DeviceType::preferred(preference);
    let mut chosen: Option<Candidate> = None;
    let mut rank = 0;
    for (adapter, desc) in adapters {
        if software(desc) {
            continue;
        }
        if chosen
            .as_ref()
            .is_some_and(|held| held.info.device_type == preferred)
        {
            break;
        }
        let Some(candidate) = candidate(adapter, desc)? else {
            continue;
        };
        let candidate_rank = candidate.info.device_type.rank(preference);
        if candidate_rank > rank {
            rank = candidate_rank;
            chosen = Some(candidate);
        }
    }
    Ok(chosen)
}

fn select(
    factory: &IDXGIFactory1,
    policy: AdapterPolicy,
) -> Result<(Option<Candidate>, Vec<AdapterInfo>), String> {
    let enumerated = adapters(factory, policy)?
        .into_iter()
        .map(|adapter| {
            let desc = unsafe { adapter.GetDesc1() }
                .map_err(|error| format!("querying a DXGI adapter: {error}"))?;
            Ok((adapter, desc))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let offered = enumerated.iter().map(|(_, desc)| describe(desc)).collect();
    let chosen = match policy {
        AdapterPolicy::Identity(wanted) => by_identity(&enumerated, wanted)?,
        AdapterPolicy::Power(preference) => by_power(&enumerated, preference)?,
    };
    Ok((chosen, offered))
}

impl Device {
    pub(crate) fn open(
        policy: AdapterPolicy,
        window: Option<WindowHandle>,
    ) -> Result<(Arc<Self>, AdapterInfo, Limits, Capability), DeviceFailure> {
        compiler().map_err(|error| format!("loading the D3D12 compute compiler: {error}"))?;
        let factory: IDXGIFactory1 =
            unsafe { CreateDXGIFactory1() }.map_err(|error| format!("creating DXGI: {error}"))?;
        let (candidate, offered) = select(&factory, policy)
            .map_err(|error| format!("enumerating DXGI adapters: {error}"))?;
        let Some(candidate) = candidate else {
            return Err(match policy {
                AdapterPolicy::Identity(wanted)
                    if offered.iter().any(|adapter| adapter.id == wanted) =>
                {
                    DeviceFailure::reason(format!(
                        "the requested adapter {wanted} cannot run compute shader model 6.0"
                    ))
                }
                AdapterPolicy::Identity(_) => DeviceFailure::missing(offered),
                AdapterPolicy::Power(_) => {
                    DeviceFailure::reason("no D3D12 adapter runs compute shader model 6.0")
                }
            });
        };
        let device = Self::assemble(candidate.device, None, None, window)?;
        Ok((
            device,
            candidate.info,
            candidate.limits,
            Capability {
                cooperative_matrix: None,
                bindless: true,
            },
        ))
    }

    pub(crate) fn borrow(
        raw: &ID3D12Device,
        advance: Option<HostAdvance>,
        keep_alive: Option<Arc<dyn Any + Send + Sync>>,
    ) -> Result<(Arc<Self>, AdapterInfo, Limits, Capability), DeviceFailure> {
        let mut lent = LENT
            .lock()
            .expect("the borrowed D3D12 devices are never poisoned");
        lent.retain(|entry| entry.native.strong_count() > 0);
        let identity = Interface::as_raw(raw) as usize;
        if let Some(entry) = lent.iter().find(|entry| entry.device == identity)
            && let Some(device) = entry.native.upgrade()
        {
            device.gate.advancing(advance);
            return Ok((
                device,
                entry.info.clone(),
                entry.limits,
                Capability {
                    cooperative_matrix: None,
                    bindless: true,
                },
            ));
        }
        compiler().map_err(|error| format!("loading the D3D12 compute compiler: {error}"))?;
        if !shader_model(raw) {
            return Err(DeviceFailure::reason("it runs no compute shader model 6.0"));
        }
        let info = describe_device(raw)?;
        let limits = limits(raw);
        let device = Self::assemble(raw.clone(), advance, keep_alive, None)?;
        lent.push(Lent {
            device: identity,
            native: Arc::downgrade(&device),
            info: info.clone(),
            limits,
        });
        Ok((
            device,
            info,
            limits,
            Capability {
                cooperative_matrix: None,
                bindless: true,
            },
        ))
    }

    fn assemble(
        raw: ID3D12Device,
        advance: Option<HostAdvance>,
        keep_alive: Option<Arc<dyn Any + Send + Sync>>,
        window: Option<WindowHandle>,
    ) -> Result<Arc<Device>, DeviceFailure> {
        let queue: ID3D12CommandQueue = unsafe {
            raw.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })
        }
        .map_err(|error| format!("creating a D3D12 queue: {error}"))?;
        let fence: ID3D12Fence = unsafe { raw.CreateFence(0, D3D12_FENCE_FLAG_NONE) }
            .map_err(|error| format!("creating a D3D12 completion fence: {error}"))?;
        let event = CompletionEvent(
            unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
                .map_err(|error| format!("creating a D3D12 completion event: {error}"))?,
        );
        let zero = allocate(&raw, 64 << 10, D3D12_HEAP_TYPE_UPLOAD, false);
        let mut mapped = ptr::null_mut();
        unsafe {
            zero.raw.Map(
                0,
                Some(&D3D12_RANGE { Begin: 0, End: 0 }),
                Some(&raw mut mapped),
            )
        }
        .map_err(|error| format!("mapping the D3D12 zero buffer: {error}"))?;
        unsafe {
            ptr::write_bytes(mapped.cast::<u8>(), 0, 64 << 10);
            zero.raw.Unmap(
                0,
                Some(&D3D12_RANGE {
                    Begin: 0,
                    End: 64 << 10,
                }),
            );
        }
        let clock = allocate(
            &raw,
            16 * FRAMES_IN_FLIGHT as u64,
            D3D12_HEAP_TYPE_READBACK,
            false,
        );
        let mut stamps = None;
        unsafe {
            raw.CreateQueryHeap(
                &D3D12_QUERY_HEAP_DESC {
                    Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                    Count: 2 * FRAMES_IN_FLIGHT as u32,
                    NodeMask: 0,
                },
                &raw mut stamps,
            )
        }
        .map_err(|error| format!("creating the D3D12 timestamp heap: {error}"))?;
        let stamps = stamps.expect("a created D3D12 timestamp heap exists");
        let frequency = unsafe { queue.GetTimestampFrequency() }
            .map_err(|error| format!("reading the D3D12 timestamp frequency: {error}"))?;
        let srv = Arc::new(DescriptorHeap::new(
            &raw,
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            16384,
            "shader resource view",
        ));
        let samplers = Arc::new(DescriptorHeap::new(
            &raw,
            D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
            1024,
            "sampler",
        ));
        Ok(Arc::new(Device {
            raw,
            queue,
            fence,
            event,
            zero,
            stamps,
            clock,
            srv,
            samplers,
            frequency,
            state: Mutex::new(QueueState::default()),
            gate: Gate::of(advance),
            window: window.and_then(|window| match window {
                WindowHandle::Windows { window, .. } => Some(window),
                _ => None,
            }),
            _keep_alive: keep_alive,
        }))
    }

    pub(crate) fn has_surface(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn create_surface(self: &Arc<Self>, width: u32, height: u32) -> Surface {
        let window = self
            .window
            .expect("a D3D12 swapchain belongs to a window device");
        let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1() }
            .unwrap_or_else(|error| panic!("creating the DXGI factory of a swapchain: {error}"));
        let description = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: BOOL::from(false),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: SWAPCHAIN_BUFFERS,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: 0,
        };
        let chain = unsafe {
            factory.CreateSwapChainForHwnd(
                &self.queue,
                HWND(window as *mut c_void),
                &description,
                None,
                None,
            )
        }
        .unwrap_or_else(|error| panic!("creating a {width} by {height} DXGI swapchain: {error}"));
        let raw: IDXGISwapChain3 = chain
            .cast()
            .expect("a DXGI swapchain is a third generation swapchain");
        let buffers = (0..SWAPCHAIN_BUFFERS)
            .map(|index| {
                let raw: ID3D12Resource =
                    unsafe { chain.GetBuffer(index) }.unwrap_or_else(|error| {
                        panic!("reading back buffer {index} of a DXGI swapchain: {error}")
                    });
                self.swapchain_texture(raw, width, height)
            })
            .collect();
        Surface {
            device: self.clone(),
            raw,
            buffers,
            width,
            height,
        }
    }

    fn swapchain_texture(&self, raw: ID3D12Resource, width: u32, height: u32) -> Texture {
        let format = TextureFormat::Bgra8Unorm;
        let target = self.allocate_target(&raw, width, height, dxgi_view(format));
        let desc = unsafe { raw.GetDesc() };
        Texture {
            resource: Arc::new(TextureResource {
                raw,
                state: Mutex::new(D3D12_RESOURCE_STATE_PRESENT),
                target: Some(target),
                srv: None,
                desc,
                width,
                height,
                format,
                usages: TextureUsages::RENDER_ATTACHMENT,
            }),
        }
    }

    pub(crate) fn idle(&self) {
        let next = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned")
            .next;
        if next > 0 {
            self.wait(next, FRAME_TIMEOUT);
        }
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        for frame in &mut state.frames {
            frame.resources.clear();
        }
    }

    pub(crate) fn compile(
        &self,
        pipeline: &Pipeline,
        program: &ComputeProgram,
        artifacts: &ArtifactCache,
    ) {
        pipeline.resource.compiled.get_or_init(|| {
            let (source, entry, key) = hlsl(program);
            if let Some(bytes) = artifacts.load(&key) {
                if let Ok(compiled) = self.pipeline(pipeline, &bytes) {
                    return compiled;
                }
                artifacts.discard(&key);
            }
            let bytes = dxil(&source, &entry, COMPUTE_PROFILE, program.label());
            artifacts.store(&key, &bytes);
            self.pipeline(pipeline, &bytes).unwrap_or_else(|error| {
                panic!(
                    "creating the D3D12 compute pipeline of {}: {error}",
                    program.label()
                )
            })
        });
    }

    fn pipeline(
        &self,
        pipeline: &Pipeline,
        bytes: &[u8],
    ) -> windows::core::Result<ID3D12PipelineState> {
        let state = D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: ManuallyDrop::new(Some(pipeline.resource.root.clone())),
            CS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: bytes.as_ptr().cast(),
                BytecodeLength: bytes.len(),
            },
            ..Default::default()
        };
        let compiled = unsafe { self.raw.CreateComputePipelineState(&state) };
        let mut state = state;
        unsafe { ManuallyDrop::drop(&mut state.pRootSignature) };
        compiled
    }

    pub(crate) fn create_buffer(
        self: &Arc<Self>,
        _label: &str,
        size: u64,
        usage: BufferUsages,
    ) -> Buffer {
        Buffer {
            resource: allocate(
                &self.raw,
                size,
                if usage.contains(BufferUsages::MAP_WRITE) {
                    D3D12_HEAP_TYPE_UPLOAD
                } else if usage.contains(BufferUsages::MAP_READ) {
                    D3D12_HEAP_TYPE_READBACK
                } else {
                    D3D12_HEAP_TYPE_DEFAULT
                },
                usage.contains(BufferUsages::STORAGE),
            ),
        }
    }

    pub(crate) fn create_pipeline(self: &Arc<Self>, program: &ComputeProgram) -> Pipeline {
        let root = root_signature(&self.raw, program.bindings(), program.label());
        Pipeline {
            resource: Arc::new(PipelineResource {
                root,
                compiled: OnceLock::new(),
            }),
        }
    }

    pub(crate) fn create_graphics_pipeline(
        self: &Arc<Self>,
        program: &GraphicsProgram,
        format: TextureFormat,
        state: PipelineState,
    ) -> GraphicsPipeline {
        let root = root_signature(&self.raw, program.bindings(), program.label());
        GraphicsPipeline {
            resource: Arc::new(GraphicsPipelineResource {
                root,
                format,
                state,
                compiled: OnceLock::new(),
                signature: OnceLock::new(),
            }),
        }
    }

    pub(crate) fn create_texture_table(
        &self,
        capacity: u32,
        fallback: &crate::Texture,
    ) -> TextureTable {
        let start = self.srv.allocate_range(capacity, "texture table");
        let table = TextureTable {
            raw: self.raw.clone(),
            heap: self.srv.clone(),
            start,
            count: capacity,
        };
        for index in 0..capacity {
            table.fill(index, fallback);
        }
        table
    }

    pub(crate) fn fill_texture_table(
        &self,
        table: &TextureTable,
        index: u32,
        texture: &crate::Texture,
    ) {
        table.fill(index, texture);
    }

    pub(crate) fn create_sampler_table(
        &self,
        capacity: u32,
        fallback: &crate::Sampler,
    ) -> SamplerTable {
        let start = self.samplers.allocate_range(capacity, "sampler table");
        let table = SamplerTable {
            raw: self.raw.clone(),
            heap: self.samplers.clone(),
            start,
            count: capacity,
        };
        for index in 0..capacity {
            table.fill(index, fallback);
        }
        table
    }

    pub(crate) fn fill_sampler_table(
        &self,
        table: &SamplerTable,
        index: u32,
        sampler: &crate::Sampler,
    ) {
        table.fill(index, sampler);
    }

    pub(crate) fn create_sampler(&self, desc: &SamplerDesc, anisotropy: u32) -> Sampler {
        let slot = self.samplers.allocate("sampler");
        unsafe {
            self.raw.CreateSampler(
                &sampler_description(desc, anisotropy),
                self.samplers.cpu(slot),
            )
        };
        Sampler {
            heap: self.samplers.clone(),
            slot,
            desc: *desc,
            anisotropy,
        }
    }

    pub(crate) fn create_texture(
        self: &Arc<Self>,
        _label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Texture {
        let declared = dxgi_format(format);
        let viewed = dxgi_view(format);
        let render_attachment = usages.contains(TextureUsages::RENDER_ATTACHMENT);
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: u64::from(width),
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: mip_levels as u16,
            Format: declared,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: if render_attachment {
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
            } else {
                D3D12_RESOURCE_FLAG_NONE
            },
        };
        let clear = D3D12_CLEAR_VALUE {
            Format: viewed,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                Color: [0.0, 0.0, 0.0, 1.0],
            },
        };
        let properties = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
            ..Default::default()
        };
        let initial = if render_attachment || usages.contains(TextureUsages::COPY_SRC) {
            D3D12_RESOURCE_STATE_COPY_SOURCE
        } else {
            D3D12_RESOURCE_STATE_COMMON
        };
        let clear = render_attachment.then_some(&clear as *const D3D12_CLEAR_VALUE);
        let mut raw = None;
        unsafe {
            self.raw.CreateCommittedResource(
                &properties,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                initial,
                clear,
                &mut raw,
            )
        }
        .unwrap_or_else(|error| {
            panic!("allocating a D3D12 texture of {width} by {height}: {error}")
        });
        let raw = raw.expect("a committed D3D12 texture exists");
        let target = render_attachment.then(|| self.allocate_target(&raw, width, height, viewed));
        let srv = usages.contains(TextureUsages::SAMPLED).then(|| {
            let slot = self.srv.allocate("shader resource view");
            unsafe {
                self.raw.CreateShaderResourceView(
                    &raw,
                    Some(&srv_desc(viewed, mip_levels)),
                    self.srv.cpu(slot),
                )
            };
            (self.srv.clone(), slot)
        });
        Texture {
            resource: Arc::new(TextureResource {
                raw,
                state: Mutex::new(initial),
                target,
                srv,
                desc,
                width,
                height,
                format,
                usages,
            }),
        }
    }

    fn allocate_target(
        &self,
        raw: &ID3D12Resource,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Target {
        let heap: ID3D12DescriptorHeap = unsafe {
            self.raw.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 1,
            })
        }
        .unwrap_or_else(|error| panic!("creating a D3D12 render target heap: {error}"));
        let view = unsafe { heap.GetCPUDescriptorHandleForHeapStart() };
        unsafe {
            self.raw.CreateRenderTargetView(
                raw,
                Some(&D3D12_RENDER_TARGET_VIEW_DESC {
                    Format: format,
                    ViewDimension: D3D12_RTV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D12_RENDER_TARGET_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_RTV {
                            MipSlice: 0,
                            PlaneSlice: 0,
                        },
                    },
                }),
                view,
            )
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: u64::from(width),
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DEPTH_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
        };
        let clear = D3D12_CLEAR_VALUE {
            Format: DEPTH_FORMAT,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                    Depth: 1.0,
                    Stencil: 0,
                },
            },
        };
        let properties = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
            ..Default::default()
        };
        let mut depth = None;
        unsafe {
            self.raw.CreateCommittedResource(
                &properties,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
                Some(&clear),
                &mut depth,
            )
        }
        .unwrap_or_else(|error| {
            panic!("allocating a D3D12 depth target of {width} by {height}: {error}")
        });
        let depth = depth.expect("a committed D3D12 depth target exists");
        let depth_heap: ID3D12DescriptorHeap = unsafe {
            self.raw.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 1,
            })
        }
        .unwrap_or_else(|error| panic!("creating a D3D12 depth target heap: {error}"));
        let depth_view = unsafe { depth_heap.GetCPUDescriptorHandleForHeapStart() };
        unsafe { self.raw.CreateDepthStencilView(&depth, None, depth_view) };
        Target {
            _heap: heap,
            view,
            depth,
            depth_state: Mutex::new(D3D12_RESOURCE_STATE_DEPTH_WRITE),
            _depth_heap: depth_heap,
            depth_view,
        }
    }

    pub(crate) fn compile_graphics(
        &self,
        pipeline: &GraphicsPipeline,
        program: &GraphicsProgram,
        artifacts: &ArtifactCache,
    ) {
        pipeline.resource.compiled.get_or_init(|| {
            let GraphicsTranslation::Hlsl {
                source,
                vertex,
                fragment,
            } = program.translate(Backend::Dx12)
            else {
                panic!("D3D12 accepts HLSL graphics programs");
            };
            let code = |entry: &str, profile: &str| {
                let mut parts = vec![source.as_bytes(), entry.as_bytes(), profile.as_bytes()];
                parts.extend(DXC_FLAGS.iter().map(|flag| flag.as_bytes()));
                let key = format!("dx12/graphics-{profile}-{}.dxil", fingerprint(&parts));
                if let Some(bytes) = artifacts.load(&key) {
                    return bytes;
                }
                let bytes = dxil(&source, entry, profile, program.label());
                artifacts.store(&key, &bytes);
                bytes
            };
            let vertex_code = code(&vertex, "vs_6_0");
            let fragment_code = code(&fragment, "ps_6_0");
            let state = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: ManuallyDrop::new(Some(pipeline.resource.root.clone())),
                VS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: vertex_code.as_ptr().cast(),
                    BytecodeLength: vertex_code.len(),
                },
                PS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: fragment_code.as_ptr().cast(),
                    BytecodeLength: fragment_code.len(),
                },
                BlendState: D3D12_BLEND_DESC {
                    RenderTarget: [render_target_blend(pipeline.resource.state); 8],
                    ..Default::default()
                },
                SampleMask: u32::MAX,
                RasterizerState: D3D12_RASTERIZER_DESC {
                    FillMode: D3D12_FILL_MODE_SOLID,
                    CullMode: D3D12_CULL_MODE_NONE,
                    FrontCounterClockwise: false.into(),
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                    DepthEnable: (!matches!(pipeline.resource.state.depth, DepthState::Off)).into(),
                    DepthWriteMask: match pipeline.resource.state.depth {
                        DepthState::TestWrite => D3D12_DEPTH_WRITE_MASK_ALL,
                        DepthState::Test | DepthState::Off => D3D12_DEPTH_WRITE_MASK_ZERO,
                    },
                    DepthFunc: D3D12_COMPARISON_FUNC_LESS,
                    StencilEnable: false.into(),
                    ..Default::default()
                },
                InputLayout: D3D12_INPUT_LAYOUT_DESC::default(),
                PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                NumRenderTargets: 1,
                RTVFormats: {
                    let mut formats = [DXGI_FORMAT_UNKNOWN; 8];
                    formats[0] = dxgi_view(pipeline.resource.format);
                    formats
                },
                DSVFormat: DEPTH_FORMAT,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                ..Default::default()
            };
            let compiled = unsafe { self.raw.CreateGraphicsPipelineState(&state) };
            let mut state = state;
            unsafe { ManuallyDrop::drop(&mut state.pRootSignature) };
            compiled.unwrap_or_else(|error| {
                panic!(
                    "creating the D3D12 graphics pipeline of {}: {error}{}",
                    program.label(),
                    debug_messages(&self.raw)
                )
            })
        });
        pipeline
            .resource
            .signature
            .get_or_init(|| command_signature(&self.raw));
    }

    fn transition_texture(
        &self,
        list: &ID3D12GraphicsCommandList,
        texture: &TextureResource,
        next: D3D12_RESOURCE_STATES,
    ) {
        self.barrier(list, &texture.raw, &texture.state, next);
    }

    fn transition_depth(
        &self,
        list: &ID3D12GraphicsCommandList,
        texture: &TextureResource,
        next: D3D12_RESOURCE_STATES,
    ) {
        let target = texture
            .target
            .as_ref()
            .expect("a depth target belongs to a render target");
        self.barrier(list, &target.depth, &target.depth_state, next);
    }

    fn barrier(
        &self,
        list: &ID3D12GraphicsCommandList,
        resource: &ID3D12Resource,
        state: &Mutex<D3D12_RESOURCE_STATES>,
        next: D3D12_RESOURCE_STATES,
    ) {
        let mut state = state
            .lock()
            .expect("a D3D12 resource state is never poisoned");
        if *state == next {
            return;
        }
        let mut barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: ManuallyDrop::new(Some(resource.clone())),
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: *state,
                    StateAfter: next,
                }),
            },
        };
        unsafe {
            list.ResourceBarrier(&[barrier.clone()]);
            ManuallyDrop::drop(&mut (*barrier.Anonymous.Transition).pResource);
        }
        *state = next;
    }

    fn transition(
        &self,
        list: &ID3D12GraphicsCommandList,
        buffer: &BufferResource,
        next: D3D12_RESOURCE_STATES,
    ) {
        let state = buffer
            .state
            .lock()
            .expect("a D3D12 resource state is never poisoned");
        if *state == D3D12_RESOURCE_STATE_GENERIC_READ && next == D3D12_RESOURCE_STATE_COPY_SOURCE {
            return;
        }
        drop(state);
        self.barrier(list, &buffer.raw, &buffer.state, next);
    }

    fn uav_barrier(&self, list: &ID3D12GraphicsCommandList) {
        let barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                UAV: ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                    pResource: ManuallyDrop::new(None),
                }),
            },
        };
        unsafe { list.ResourceBarrier(&[barrier]) };
    }

    pub(crate) fn submit(
        &self,
        writes: &[Write],
        commands: &[Command],
        first_dispatch: bool,
    ) -> u64 {
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        self.retire(&mut state);
        if writes.is_empty() && commands.is_empty() {
            return state.next;
        }
        let slot = self.acquire(&mut state);
        state.next = state
            .next
            .checked_add(1)
            .expect("compute submission indices fit in u64");
        let index = state.next;
        let frame = &mut state.frames[slot];
        frame.begin();
        frame.index = index;
        frame.first_dispatch = first_dispatch;
        let list = frame.list.clone();
        unsafe {
            list.SetDescriptorHeaps(&[Some(self.srv.raw.clone()), Some(self.samplers.raw.clone())]);
            list.EndQuery(&self.stamps, D3D12_QUERY_TYPE_TIMESTAMP, (slot * 2) as u32);
        }
        for write in writes {
            let (upload, offset) = frame.stage(&self.raw, &write.bytes);
            let target = native_buffer(&write.buffer);
            self.transition(&frame.list, target, D3D12_RESOURCE_STATE_COPY_DEST);
            unsafe {
                frame.list.CopyBufferRegion(
                    &target.raw,
                    write.offset,
                    &upload,
                    offset,
                    write.bytes.len() as u64,
                )
            };
            frame.resources.push(target.clone());
        }
        let mut presenting = None;
        for operation in commands {
            match operation {
                Command::Copy {
                    source,
                    source_offset,
                    target,
                    target_offset,
                    bytes,
                } => {
                    let source = buffer(source);
                    let target = buffer(target);
                    self.transition(&list, source, D3D12_RESOURCE_STATE_COPY_SOURCE);
                    self.transition(&list, target, D3D12_RESOURCE_STATE_COPY_DEST);
                    unsafe {
                        list.CopyBufferRegion(
                            &target.raw,
                            *target_offset,
                            &source.raw,
                            *source_offset,
                            *bytes,
                        )
                    };
                    frame.resources.push(source.clone());
                    frame.resources.push(target.clone());
                }
                Command::Clear {
                    buffer: target,
                    offset,
                    bytes,
                } => {
                    let target = buffer(target);
                    self.transition(&list, target, D3D12_RESOURCE_STATE_COPY_DEST);
                    let mut written = 0;
                    while written < *bytes {
                        let chunk = (64 << 10).min(bytes - written);
                        unsafe {
                            list.CopyBufferRegion(
                                &target.raw,
                                offset + written,
                                &self.zero.raw,
                                0,
                                chunk,
                            )
                        };
                        written += chunk;
                    }
                    self.uav_barrier(&list);
                    frame.resources.push(target.clone());
                }
                Command::Render {
                    target,
                    clear,
                    draws,
                } => {
                    let texture = native_texture(target.native());
                    let target_frame = texture
                        .target
                        .as_ref()
                        .expect("a render target holds its views");
                    self.transition_texture(&list, texture, D3D12_RESOURCE_STATE_RENDER_TARGET);
                    self.transition_depth(&list, texture, D3D12_RESOURCE_STATE_DEPTH_WRITE);
                    unsafe {
                        list.OMSetRenderTargets(
                            1,
                            Some(&target_frame.view),
                            false,
                            Some(&target_frame.depth_view),
                        );
                        list.ClearRenderTargetView(target_frame.view, clear, None);
                        list.ClearDepthStencilView(
                            target_frame.depth_view,
                            D3D12_CLEAR_FLAG_DEPTH,
                            1.0,
                            0,
                            None,
                        );
                    }
                    for draw in draws {
                        let pipeline = graphics_pipeline(&draw.pipeline.native);
                        unsafe {
                            list.RSSetViewports(&[D3D12_VIEWPORT {
                                TopLeftX: draw.viewport.x,
                                TopLeftY: draw.viewport.y,
                                Width: draw.viewport.width,
                                Height: draw.viewport.height,
                                MinDepth: 0.0,
                                MaxDepth: 1.0,
                            }]);
                            list.RSSetScissorRects(&[RECT {
                                left: draw.scissor.x as i32,
                                top: draw.scissor.y as i32,
                                right: (draw.scissor.x + draw.scissor.width) as i32,
                                bottom: (draw.scissor.y + draw.scissor.height) as i32,
                            }]);
                            list.SetGraphicsRootSignature(&pipeline.root);
                            list.SetPipelineState(
                                pipeline
                                    .compiled
                                    .get()
                                    .expect("a D3D12 graphics pipeline is compiled before a draw"),
                            );
                            list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                        }
                        for group in &draw.groups {
                            claim_group(&group.native);
                            let mut resources = group.resources.iter();
                            let specs = draw
                                .pipeline
                                .program
                                .bindings()
                                .iter()
                                .enumerate()
                                .filter(|(_, spec)| spec.group == group.group);
                            for (root, spec) in specs {
                                let resource = resources
                                    .next()
                                    .expect("a bind group fills every slot of its group");
                                match resource {
                                    BoundResource::Buffer(binding) => {
                                        let target = buffer(&binding.buffer);
                                        let state = if spec.kind.writable() {
                                            D3D12_RESOURCE_STATE_UNORDERED_ACCESS
                                        } else {
                                            D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE
                                        };
                                        self.transition(&list, target, state);
                                        let address = unsafe { target.raw.GetGPUVirtualAddress() }
                                            + binding.offset;
                                        unsafe {
                                            if spec.kind.writable() {
                                                list.SetGraphicsRootUnorderedAccessView(
                                                    root as u32,
                                                    address,
                                                );
                                            } else {
                                                list.SetGraphicsRootShaderResourceView(
                                                    root as u32,
                                                    address,
                                                );
                                            }
                                        }
                                        frame.resources.push(target.clone());
                                    }
                                    BoundResource::Texture(texture) => {
                                        let texture = native_texture(texture.native());
                                        self.transition_texture(
                                            &list,
                                            texture,
                                            D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE,
                                        );
                                        let (heap, slot) = texture.srv.as_ref().expect(
                                            "a sampled texture owns its shader resource view",
                                        );
                                        unsafe {
                                            list.SetGraphicsRootDescriptorTable(
                                                root as u32,
                                                heap.gpu(*slot),
                                            )
                                        };
                                        frame.resources.push(texture.clone());
                                    }
                                    BoundResource::Sampler(sampler) => {
                                        let sampler = native_sampler(sampler.native());
                                        unsafe {
                                            list.SetGraphicsRootDescriptorTable(
                                                root as u32,
                                                sampler.heap.gpu(sampler.slot),
                                            )
                                        };
                                    }
                                    BoundResource::TextureTable(table) => {
                                        for texture in table.entries() {
                                            let texture = native_texture(texture.native());
                                            self.transition_texture(
                                                &list,
                                                texture,
                                                D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE,
                                            );
                                            frame.resources.push(texture.clone());
                                        }
                                        unsafe {
                                            list.SetGraphicsRootDescriptorTable(
                                                root as u32,
                                                texture_table(table.native()).gpu(),
                                            )
                                        };
                                    }
                                    BoundResource::SamplerTable(table) => unsafe {
                                        list.SetGraphicsRootDescriptorTable(
                                            root as u32,
                                            sampler_table(table.native()).gpu(),
                                        )
                                    },
                                }
                            }
                        }
                        match &draw.count {
                            RecordedCount::Direct {
                                vertices,
                                instances,
                            } => unsafe {
                                list.DrawInstanced(*vertices, *instances, 0, 0);
                            },
                            RecordedCount::Indirect { arguments, offset } => {
                                let arguments = buffer(arguments);
                                self.transition(
                                    &list,
                                    arguments,
                                    D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
                                );
                                unsafe {
                                    list.ExecuteIndirect(
                                        pipeline.signature.get().expect(
                                            "a D3D12 graphics pipeline owns an indirect signature",
                                        ),
                                        1,
                                        &arguments.raw,
                                        *offset,
                                        None,
                                        0,
                                    );
                                }
                                frame.resources.push(arguments.clone());
                            }
                        }
                        frame.resources.push(pipeline.clone());
                    }
                    self.uav_barrier(&list);
                    self.transition_texture(&list, texture, D3D12_RESOURCE_STATE_COPY_SOURCE);
                    frame.resources.push(texture.clone());
                }
                Command::WriteTexture {
                    texture,
                    level,
                    bytes,
                } => {
                    let texture = native_texture(texture.native());
                    let (upload, offset) = frame.stage(&self.raw, bytes);
                    self.transition_texture(&list, texture, D3D12_RESOURCE_STATE_COPY_DEST);
                    let width = (texture.width >> level).max(1);
                    let height = (texture.height >> level).max(1);
                    let row = u64::from(width) * u64::from(texture.format.bytes_per_texel());
                    let pitch = row.next_multiple_of(256) as u32;
                    let destination = D3D12_TEXTURE_COPY_LOCATION {
                        pResource: ManuallyDrop::new(Some(texture.raw.clone())),
                        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                            SubresourceIndex: *level,
                        },
                    };
                    let source = D3D12_TEXTURE_COPY_LOCATION {
                        pResource: ManuallyDrop::new(Some(upload)),
                        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                            PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                                Offset: offset,
                                Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                                    Format: texture.desc.Format,
                                    Width: width,
                                    Height: height,
                                    Depth: 1,
                                    RowPitch: pitch,
                                },
                            },
                        },
                    };
                    unsafe {
                        list.CopyTextureRegion(&destination, 0, 0, 0, &source, None);
                    }
                    let mut destination = destination;
                    let mut source = source;
                    unsafe {
                        ManuallyDrop::drop(&mut destination.pResource);
                        ManuallyDrop::drop(&mut source.pResource);
                    }
                    let next = if texture.usages.contains(TextureUsages::SAMPLED) {
                        D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE
                    } else if texture.usages.contains(TextureUsages::COPY_SRC)
                        || texture.usages.contains(TextureUsages::RENDER_ATTACHMENT)
                    {
                        D3D12_RESOURCE_STATE_COPY_SOURCE
                    } else {
                        D3D12_RESOURCE_STATE_COPY_DEST
                    };
                    self.transition_texture(&list, texture, next);
                    frame.resources.push(texture.clone());
                }
                Command::CopyTexture {
                    texture,
                    target,
                    target_offset,
                } => {
                    let texture = native_texture(texture.native());
                    let target = buffer(target);
                    self.transition_texture(&list, texture, D3D12_RESOURCE_STATE_COPY_SOURCE);
                    self.transition(&list, target, D3D12_RESOURCE_STATE_COPY_DEST);
                    let mut layout = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
                    let mut rows = 0u32;
                    let mut row_bytes = 0u64;
                    let mut total = 0u64;
                    unsafe {
                        self.raw.GetCopyableFootprints(
                            &texture.desc,
                            0,
                            1,
                            *target_offset,
                            Some(&mut layout),
                            Some(&mut rows),
                            Some(&mut row_bytes),
                            Some(&mut total),
                        )
                    };
                    assert_eq!(
                        row_bytes,
                        u64::from(texture.width) * u64::from(texture.format.bytes_per_texel()),
                        "a D3D12 texture row spans {row_bytes} bytes where {} texels span {}",
                        texture.width,
                        u64::from(texture.width) * u64::from(texture.format.bytes_per_texel()),
                    );
                    assert_eq!(
                        u64::from(layout.Footprint.RowPitch),
                        (u64::from(texture.width) * u64::from(texture.format.bytes_per_texel()))
                            .next_multiple_of(256),
                        "a D3D12 texture row is padded to {} bytes",
                        layout.Footprint.RowPitch,
                    );
                    assert_eq!(
                        total,
                        (u64::from(texture.height) - 1) * u64::from(layout.Footprint.RowPitch)
                            + row_bytes,
                        "a D3D12 texture readback spans {total} bytes"
                    );
                    assert_eq!(
                        layout.Offset, *target_offset,
                        "a D3D12 texture readback starts at the requested offset"
                    );
                    let destination = D3D12_TEXTURE_COPY_LOCATION {
                        pResource: ManuallyDrop::new(Some(target.raw.clone())),
                        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                            PlacedFootprint: layout,
                        },
                    };
                    let source = D3D12_TEXTURE_COPY_LOCATION {
                        pResource: ManuallyDrop::new(Some(texture.raw.clone())),
                        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                            SubresourceIndex: 0,
                        },
                    };
                    unsafe {
                        list.CopyTextureRegion(&destination, 0, 0, 0, &source, None);
                    }
                    let mut destination = destination;
                    let mut source = source;
                    unsafe {
                        ManuallyDrop::drop(&mut destination.pResource);
                        ManuallyDrop::drop(&mut source.pResource);
                    }
                    frame.resources.push(texture.clone());
                    frame.resources.push(target.clone());
                }
                Command::Present { texture, native } => {
                    let present = native.dx12();
                    let texture = native_texture(texture.native());
                    self.transition_texture(&list, texture, D3D12_RESOURCE_STATE_PRESENT);
                    frame.resources.push(texture.clone());
                    presenting = Some(present);
                }
                Command::Dispatch {
                    pipeline: handle,
                    group,
                    groups,
                } => {
                    claim_group(&group.native);
                    let pipeline = pipeline(&handle.slot.native);
                    unsafe {
                        list.SetComputeRootSignature(&pipeline.root);
                        list.SetPipelineState(
                            pipeline
                                .compiled
                                .get()
                                .expect("a D3D12 pipeline is compiled before dispatch"),
                        );
                    }
                    let mut resources = group.resources.iter();
                    for (root, spec) in handle.slot.program.bindings().iter().enumerate() {
                        let resource = resources
                            .next()
                            .expect("a bind group fills every slot of its group");
                        match resource {
                            BoundResource::Buffer(binding) => {
                                let target = buffer(&binding.buffer);
                                let state = if spec.kind.writable() {
                                    D3D12_RESOURCE_STATE_UNORDERED_ACCESS
                                } else {
                                    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
                                };
                                self.transition(&list, target, state);
                                let address =
                                    unsafe { target.raw.GetGPUVirtualAddress() } + binding.offset;
                                unsafe {
                                    if spec.kind.writable() {
                                        list.SetComputeRootUnorderedAccessView(
                                            root as u32,
                                            address,
                                        );
                                    } else {
                                        list.SetComputeRootShaderResourceView(root as u32, address);
                                    }
                                }
                                frame.resources.push(target.clone());
                            }
                            BoundResource::Texture(texture) => {
                                let texture = native_texture(texture.native());
                                self.transition_texture(
                                    &list,
                                    texture,
                                    D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE,
                                );
                                let (heap, slot) = texture
                                    .srv
                                    .as_ref()
                                    .expect("a sampled texture owns its shader resource view");
                                unsafe {
                                    list.SetComputeRootDescriptorTable(root as u32, heap.gpu(*slot))
                                };
                                frame.resources.push(texture.clone());
                            }
                            BoundResource::Sampler(sampler) => {
                                let sampler = native_sampler(sampler.native());
                                unsafe {
                                    list.SetComputeRootDescriptorTable(
                                        root as u32,
                                        sampler.heap.gpu(sampler.slot),
                                    )
                                };
                            }
                            BoundResource::TextureTable(_) | BoundResource::SamplerTable(_) => {
                                panic!("a compute dispatch binds no resource table")
                            }
                        }
                    }
                    unsafe { list.Dispatch(groups[0], groups[1], groups[2]) };
                    self.uav_barrier(&list);
                    frame.resources.push(pipeline.clone());
                }
            }
        }
        unsafe {
            list.EndQuery(
                &self.stamps,
                D3D12_QUERY_TYPE_TIMESTAMP,
                (slot * 2 + 1) as u32,
            );
            list.ResolveQueryData(
                &self.stamps,
                D3D12_QUERY_TYPE_TIMESTAMP,
                (slot * 2) as u32,
                2,
                &self.clock.raw,
                (slot * 16) as u64,
            );
            list.Close()
        }
        .unwrap_or_else(|error| panic!("closing a D3D12 compute command list: {error}"));
        let command: ID3D12CommandList = list
            .cast()
            .expect("a compute command list is a D3D12 command list");
        let turn = self.gate.take();
        unsafe { self.queue.ExecuteCommandLists(&[Some(command)]) };
        if let Some(present) = presenting {
            present.present();
        }
        unsafe { self.queue.Signal(&self.fence, index) }
            .unwrap_or_else(|error| panic!("signaling D3D12 compute completion: {error}"));
        drop(turn);
        index
    }

    fn acquire(&self, state: &mut QueueState) -> usize {
        if let Some(slot) = state
            .frames
            .iter()
            .position(|frame| frame.index <= state.completed)
        {
            return slot;
        }
        if state.frames.len() < FRAMES_IN_FLIGHT {
            state.frames.push(Frame::new(&self.raw));
            return state.frames.len() - 1;
        }
        let slot = state
            .frames
            .iter()
            .enumerate()
            .min_by_key(|(_, frame)| frame.index)
            .map(|(slot, _)| slot)
            .expect("a queue that holds a frame recycles one");
        let index = state.frames[slot].index;
        self.await_completion(state, index, FRAME_TIMEOUT)
            .unwrap_or_else(|error| panic!("{error}"));
        slot
    }

    fn retire(&self, state: &mut QueueState) {
        let completed = unsafe { self.fence.GetCompletedValue() };
        if completed == u64::MAX {
            if let Err(error) = self.device_error() {
                panic!("{error}");
            }
            panic!("D3D12 compute fence failed");
        }
        state.completed = state.completed.max(completed.min(state.next));
        self.record(state);
    }

    fn record(&self, state: &mut QueueState) {
        for slot in 0..state.frames.len() {
            let index = state.frames[slot].index;
            if index == 0 || index > state.completed {
                continue;
            }
            let key = index as usize % TIME_SLOTS;
            if state.times[key].is_some_and(|(kept, _)| kept >= index) {
                continue;
            }
            state.times[key] = Some((index, self.read_clock(slot)));
        }
    }

    fn read_clock(&self, slot: usize) -> f64 {
        let mut mapped = ptr::null_mut();
        let range = D3D12_RANGE {
            Begin: slot * 16,
            End: slot * 16 + 16,
        };
        unsafe { self.clock.raw.Map(0, Some(&range), Some(&raw mut mapped)) }
            .unwrap_or_else(|error| panic!("mapping the D3D12 timestamp readback: {error}"));
        let stamps = unsafe { std::slice::from_raw_parts(mapped.cast::<u64>().add(slot * 2), 2) };
        let seconds = stamps[1].wrapping_sub(stamps[0]) as f64 / self.frequency as f64;
        unsafe { self.clock.raw.Unmap(0, None) };
        seconds
    }

    pub(crate) fn complete(&self, index: u64) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        self.retire(&mut state);
        index <= state.completed
    }

    pub(crate) fn seconds(&self, index: u64) -> f64 {
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        self.await_completion(&mut state, index, FRAME_TIMEOUT)
            .unwrap_or_else(|error| panic!("{error}"));
        match state.times[index as usize % TIME_SLOTS] {
            Some((kept, seconds)) if kept == index => seconds,
            _ => panic!(
                "the device time of submission {index} has left the last {TIME_SLOTS} of this queue"
            ),
        }
    }

    pub(crate) fn wait(&self, index: u64, timeout: Duration) {
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        self.await_completion(&mut state, index, timeout)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    fn await_completion(
        &self,
        state: &mut QueueState,
        index: u64,
        timeout: Duration,
    ) -> Result<(), String> {
        if index > state.next {
            return Err("waiting on an unsubmitted D3D12 command".to_owned());
        }
        self.retire(state);
        if index <= state.completed {
            return Ok(());
        }
        unsafe { self.fence.SetEventOnCompletion(index, self.event.0) }
            .map_err(|error| format!("arming the D3D12 compute fence: {error}"))?;
        let timeout = hang_deadline(
            state
                .frames
                .iter()
                .map(|frame| (frame.index, frame.first_dispatch)),
            index,
            timeout,
        );
        let millis = |span: Duration| span.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        match self.gate.advance() {
            None => {
                let result = unsafe { WaitForSingleObject(self.event.0, millis(timeout)) };
                if result != WAIT_OBJECT_0 {
                    return Err(format!(
                        "waiting for D3D12 compute work failed or timed out: {result:?}"
                    ));
                }
            }
            Some(advance) => {
                let started = Instant::now();
                loop {
                    let slice = HOST_SLICE.min(timeout.saturating_sub(started.elapsed()));
                    if unsafe { WaitForSingleObject(self.event.0, millis(slice)) } == WAIT_OBJECT_0
                    {
                        break;
                    }
                    if started.elapsed() >= timeout {
                        return Err("waiting for D3D12 compute work failed or timed out".to_owned());
                    }
                    advance.advance();
                }
            }
        }
        self.device_error()?;
        self.retire(state);
        if state.completed < index {
            return Err("D3D12 compute did not complete".to_owned());
        }
        Ok(())
    }

    fn release(&self) {
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        let index = state.next;
        let _ = self.await_completion(&mut state, index, RELEASE_TIMEOUT);
    }

    pub(crate) fn write_host(&self, buffer: &Buffer, offset: u64, bytes: &[u8]) {
        let mut mapped = ptr::null_mut();
        unsafe {
            buffer.resource.raw.Map(
                0,
                Some(&D3D12_RANGE { Begin: 0, End: 0 }),
                Some(&raw mut mapped),
            )
        }
        .unwrap_or_else(|error| panic!("mapping a D3D12 upload: {error}"));
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                mapped.cast::<u8>().add(offset as usize),
                bytes.len(),
            );
            buffer.resource.raw.Unmap(
                0,
                Some(&D3D12_RANGE {
                    Begin: offset as usize,
                    End: (offset + bytes.len() as u64) as usize,
                }),
            );
        }
    }

    pub(crate) fn read_host(&self, buffer: &Buffer, offset: u64, bytes: u64) -> Vec<u8> {
        let mut mapped = ptr::null_mut();
        unsafe {
            buffer.resource.raw.Map(
                0,
                Some(&D3D12_RANGE {
                    Begin: offset as usize,
                    End: (offset + bytes) as usize,
                }),
                Some(&raw mut mapped),
            )
        }
        .unwrap_or_else(|error| panic!("mapping a D3D12 mirror: {error}"));
        let result = unsafe {
            std::slice::from_raw_parts(mapped.cast::<u8>().add(offset as usize), bytes as usize)
                .to_vec()
        };
        unsafe {
            buffer
                .resource
                .raw
                .Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }))
        };
        result
    }

    pub(crate) fn read(&self, buffer: &Buffer, bytes: u64) -> Vec<u8> {
        let mut mapped = ptr::null_mut();
        unsafe {
            buffer.resource.raw.Map(
                0,
                Some(&D3D12_RANGE {
                    Begin: 0,
                    End: bytes as usize,
                }),
                Some(&raw mut mapped),
            )
        }
        .unwrap_or_else(|error| panic!("mapping a D3D12 readback: {error}"));
        let result =
            unsafe { std::slice::from_raw_parts(mapped.cast::<u8>(), bytes as usize) }.to_vec();
        unsafe {
            buffer
                .resource
                .raw
                .Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }))
        };
        result
    }

    fn device_error(&self) -> Result<(), String> {
        unsafe { self.raw.GetDeviceRemovedReason() }.map_err(|error| {
            format!(
                "the D3D12 device was lost: {error}{}",
                debug_messages(&self.raw)
            )
        })
    }

    pub(crate) fn assert_alive(&self) {
        if let Err(error) = self.device_error() {
            panic!("{error}");
        }
        let mut state = self
            .state
            .lock()
            .expect("the D3D12 compute queue is never poisoned");
        self.retire(&mut state);
    }
}

fn command_signature(device: &ID3D12Device) -> ID3D12CommandSignature {
    let arguments = [D3D12_INDIRECT_ARGUMENT_DESC {
        Type: D3D12_INDIRECT_ARGUMENT_TYPE_DRAW,
        ..Default::default()
    }];
    let desc = D3D12_COMMAND_SIGNATURE_DESC {
        ByteStride: 16,
        NumArgumentDescs: 1,
        pArgumentDescs: arguments.as_ptr(),
        NodeMask: 0,
    };
    let mut signature = None;
    unsafe { device.CreateCommandSignature(&desc, None, &mut signature) }
        .unwrap_or_else(|error| panic!("creating a D3D12 indirect draw signature: {error}"));
    signature.expect("a created D3D12 command signature exists")
}

fn debug_messages(device: &ID3D12Device) -> String {
    let Ok(queue) = device.cast::<ID3D12InfoQueue>() else {
        return String::new();
    };
    let count = unsafe { queue.GetNumStoredMessages() };
    let mut messages = String::new();
    for index in count.saturating_sub(8)..count {
        let mut bytes = 0usize;
        unsafe { queue.GetMessage(index, None, &mut bytes) }.ok();
        let mut message = vec![0u8; bytes];
        unsafe { queue.GetMessage(index, Some(message.as_mut_ptr().cast()), &mut bytes) }.ok();
        let message = message.as_ptr().cast::<D3D12_MESSAGE>();
        if let Some(message) = unsafe { message.as_ref() } {
            let text = unsafe {
                std::slice::from_raw_parts(
                    message.pDescription.cast::<u8>(),
                    message.DescriptionByteLength,
                )
            };
            messages.push('\n');
            messages.push_str(&String::from_utf8_lossy(text));
        }
    }
    messages
}

fn allocate(
    raw: &ID3D12Device,
    size: u64,
    heap: D3D12_HEAP_TYPE,
    storage: bool,
) -> Arc<BufferResource> {
    let properties = D3D12_HEAP_PROPERTIES {
        Type: heap,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
        ..Default::default()
    };
    let descriptor = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: size,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: if storage {
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS
        } else {
            D3D12_RESOURCE_FLAG_NONE
        },
        ..Default::default()
    };
    let state = if heap == D3D12_HEAP_TYPE_UPLOAD {
        D3D12_RESOURCE_STATE_GENERIC_READ
    } else if heap == D3D12_HEAP_TYPE_READBACK {
        D3D12_RESOURCE_STATE_COPY_DEST
    } else {
        D3D12_RESOURCE_STATE_COMMON
    };
    let mut resource = None;
    unsafe {
        raw.CreateCommittedResource(
            &properties,
            D3D12_HEAP_FLAG_NONE,
            &descriptor,
            state,
            None,
            &mut resource,
        )
    }
    .unwrap_or_else(|error| panic!("allocating {size} D3D12 compute bytes: {error}"));
    Arc::new(BufferResource {
        raw: resource.expect("a committed buffer was created"),
        state: Mutex::new(state),
    })
}

pub(crate) fn hlsl(program: &ComputeProgram) -> (String, String, String) {
    let ShaderTranslation::Hlsl { source, entry } = program.translate(Backend::Dx12) else {
        panic!("D3D12 accepts HLSL compute programs");
    };
    let mut parts = vec![
        source.as_bytes(),
        entry.as_bytes(),
        COMPUTE_PROFILE.as_bytes(),
    ];
    parts.extend(DXC_FLAGS.iter().map(|flag| flag.as_bytes()));
    let key = format!("dx12/{}.dxil", fingerprint(&parts));
    (source, entry, key)
}

const COMPUTE_PROFILE: &str = "cs_6_0";

fn root_signature(
    device: &ID3D12Device,
    bindings: &[BindingSpec],
    label: &str,
) -> ID3D12RootSignature {
    let ranges = bindings
        .iter()
        .filter_map(|spec| match spec.kind {
            belfry_gpu_shader::BindingKind::SampledImage
            | belfry_gpu_shader::BindingKind::Sampler
            | belfry_gpu_shader::BindingKind::SampledImageTable
            | belfry_gpu_shader::BindingKind::SamplerTable => Some(D3D12_DESCRIPTOR_RANGE {
                RangeType: if matches!(
                    spec.kind,
                    belfry_gpu_shader::BindingKind::Sampler
                        | belfry_gpu_shader::BindingKind::SamplerTable
                ) {
                    D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER
                } else {
                    D3D12_DESCRIPTOR_RANGE_TYPE_SRV
                },
                NumDescriptors: spec.count,
                BaseShaderRegister: spec.binding,
                RegisterSpace: spec.group,
                OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut range = 0;
    let parameters = bindings
        .iter()
        .map(|spec| match spec.kind {
            belfry_gpu_shader::BindingKind::SampledImage
            | belfry_gpu_shader::BindingKind::Sampler
            | belfry_gpu_shader::BindingKind::SampledImageTable
            | belfry_gpu_shader::BindingKind::SamplerTable => {
                let held = &ranges[range];
                range += 1;
                D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                            NumDescriptorRanges: 1,
                            pDescriptorRanges: held,
                        },
                    },
                    ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
                }
            }
            _ => D3D12_ROOT_PARAMETER {
                ParameterType: if spec.kind.writable() {
                    D3D12_ROOT_PARAMETER_TYPE_UAV
                } else {
                    D3D12_ROOT_PARAMETER_TYPE_SRV
                },
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR {
                        ShaderRegister: spec.binding,
                        RegisterSpace: spec.group,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
        })
        .collect::<Vec<_>>();
    let desc = D3D12_ROOT_SIGNATURE_DESC {
        NumParameters: parameters.len() as u32,
        pParameters: parameters.as_ptr(),
        Flags: D3D12_ROOT_SIGNATURE_FLAG_NONE,
        ..Default::default()
    };
    let mut blob: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    if let Err(error) = unsafe {
        D3D12SerializeRootSignature(
            &desc,
            D3D_ROOT_SIGNATURE_VERSION_1,
            &mut blob,
            Some(&mut errors),
        )
    } {
        let message = errors
            .as_ref()
            .map(|blob| unsafe {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    blob.GetBufferPointer().cast::<u8>(),
                    blob.GetBufferSize(),
                ))
                .into_owned()
            })
            .unwrap_or_else(|| error.to_string());
        panic!("serializing {label}'s D3D12 root signature: {message}");
    }
    let blob = blob.expect("root signature serialization produced data");
    let bytes = unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    };
    unsafe { device.CreateRootSignature(0, bytes) }
        .unwrap_or_else(|error| panic!("creating {label}'s D3D12 root signature: {error}"))
}

impl Surface {
    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn format(&self) -> TextureFormat {
        TextureFormat::Bgra8Unorm
    }

    pub(crate) fn acquire(&mut self) -> Option<NativeFrame> {
        let index = unsafe { self.raw.GetCurrentBackBufferIndex() } as usize;
        let texture = self.buffers[index].clone();
        Some(NativeFrame {
            texture: NativeTexture::Dx12(texture),
            width: self.width,
            height: self.height,
            format: TextureFormat::Bgra8Unorm,
            present: NativePresent::Dx12(Present {
                raw: self.raw.clone(),
            }),
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.device.idle();
        self.buffers.clear();
        unsafe {
            self.raw.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )
        }
        .unwrap_or_else(|error| {
            panic!("resizing a DXGI swapchain to {width} by {height}: {error}")
        });
        self.buffers = (0..SWAPCHAIN_BUFFERS)
            .map(|index| {
                let raw: ID3D12Resource =
                    unsafe { self.raw.GetBuffer(index) }.unwrap_or_else(|error| {
                        panic!("reading back buffer {index} of a resized swapchain: {error}")
                    });
                self.device.swapchain_texture(raw, width, height)
            })
            .collect();
        self.width = width;
        self.height = height;
    }
}

impl Present {
    pub(crate) fn present(&self) {
        unsafe { self.raw.Present(1, DXGI_PRESENT(0)) }
            .ok()
            .unwrap_or_else(|error| panic!("presenting a DXGI swapchain: {error}"));
    }
}

impl GraphicsPipeline {
    pub(crate) fn is_compiled(&self) -> bool {
        self.resource.compiled.get().is_some()
    }
}

impl Pipeline {
    pub(crate) fn is_compiled(&self) -> bool {
        self.resource.compiled.get().is_some()
    }
}

fn dxil(source: &str, entry: &str, profile: &str, label: &str) -> Vec<u8> {
    let library = compiler()
        .unwrap_or_else(|error| panic!("D3D12 needs dxcompiler.dll to compile {label}: {error}"));
    type Create = unsafe extern "system" fn(
        *const GUID,
        *const GUID,
        *mut *mut c_void,
    ) -> windows::core::HRESULT;
    let create: libloading::Symbol<'_, Create> = unsafe { library.get(b"DxcCreateInstance\0") }
        .unwrap_or_else(|error| panic!("finding DXC in dxcompiler.dll: {error}"));
    let mut raw = ptr::null_mut();
    unsafe { create(&CLSID_DxcCompiler, &IDxcCompiler3::IID, &raw mut raw) }
        .ok()
        .unwrap_or_else(|error| panic!("creating the D3D12 shader compiler: {error}"));
    let compiler = unsafe { IDxcCompiler3::from_raw(raw) };
    let mut arguments = vec!["-E", entry, "-T", profile];
    arguments.extend_from_slice(DXC_FLAGS);
    let strings = arguments
        .iter()
        .map(|arg| arg.encode_utf16().chain([0]).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let arguments = strings
        .iter()
        .map(|arg| PCWSTR(arg.as_ptr()))
        .collect::<Vec<_>>();
    let result: IDxcResult = unsafe {
        compiler.Compile(
            &DxcBuffer {
                Ptr: source.as_ptr().cast(),
                Size: source.len(),
                Encoding: DXC_CP_UTF8.0,
            },
            Some(&arguments),
            None,
        )
    }
    .unwrap_or_else(|error| panic!("compiling the HLSL of {label}: {error}"));
    let status = unsafe { result.GetStatus() }.expect("DXC reports shader status");
    if let Err(error) = status.ok() {
        let mut errors: Option<IDxcBlobUtf8> = None;
        unsafe { result.GetOutput(DXC_OUT_ERRORS, ptr::null_mut(), &mut errors) }
            .expect("DXC supplies diagnostics");
        let message = errors
            .map(|blob| unsafe {
                std::str::from_utf8(std::slice::from_raw_parts(
                    blob.GetStringPointer().0.cast(),
                    blob.GetStringLength(),
                ))
                .expect("DXC diagnostics are UTF-8")
                .to_owned()
            })
            .unwrap_or_else(|| error.to_string());
        panic!("compiling the D3D12 compute shader {label}: {message}");
    }
    let mut object: Option<IDxcBlob> = None;
    unsafe { result.GetOutput(DXC_OUT_OBJECT, ptr::null_mut(), &mut object) }
        .expect("DXC supplies the compiled shader");
    let object = object.expect("the compiled D3D12 shader exists");
    unsafe {
        std::slice::from_raw_parts(
            object.GetBufferPointer().cast::<u8>(),
            object.GetBufferSize(),
        )
    }
    .to_vec()
}

impl Drop for Device {
    fn drop(&mut self) {
        self.release();
    }
}
