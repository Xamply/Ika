#include <windows.h>
#include <d3d11_4.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <dxgi1_3.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfmediaengine.h>
#include <oleauto.h>
#include <wrl.h>
#include <wrl/implements.h>
#include <wincodec.h>
#include "mpv/client.h"
#include "mpv/render.h"
#include "mpv/render_d3d11.h"
#include <algorithm>
#include <atomic>
#include <cmath>
#include <initializer_list>
#include <string>
#include <utility>
#include <vector>
#include <chrono>
#include <timeapi.h>
#include "audio_sessions.h"
#include "frame_cache.h"

using Microsoft::WRL::ComPtr;
using Microsoft::WRL::Make;
using Microsoft::WRL::RuntimeClass;
using Microsoft::WRL::RuntimeClassFlags;
using Microsoft::WRL::ClassicCom;

class Events final : public RuntimeClass<RuntimeClassFlags<ClassicCom>, IMFMediaEngineNotify> {
public:
    std::atomic<HRESULT> failure{S_OK};
    IFACEMETHODIMP EventNotify(DWORD event, DWORD_PTR, DWORD param) override {
        if (event == MF_MEDIA_ENGINE_EVENT_ERROR) {
            const auto hr = static_cast<HRESULT>(param);
            failure = FAILED(hr) ? hr : E_FAIL;
        }
        return S_OK;
    }
};

#define WIDEN_IMPL(s) L##s
#define WIDEN(s) WIDEN_IMPL(s)
#define TRY(expression) do { const HRESULT status = (expression); if (FAILED(status)) { \
    log_status(WIDEN(#expression), status); return status; } } while (false)

struct Handle {
    HANDLE value{};
    ~Handle() { if (value) CloseHandle(value); }
};
struct Player {
    ComPtr<IMFMediaEngine> engine;
    ~Player() { if (engine) engine->Shutdown(); }
};

// libmpv is loaded at runtime so the launcher can ship its selected mpv build
// beside video.exe without linking the GPL player into this small bridge.
struct MpvApi {
    HMODULE module{};
    decltype(&mpv_create) create{};
    decltype(&mpv_initialize) initialize{};
    decltype(&mpv_terminate_destroy) terminate_destroy{};
    decltype(&mpv_set_option_string) set_option_string{};
    decltype(&mpv_command) command{};
    decltype(&mpv_set_property_string) set_property_string{};
    decltype(&mpv_get_property) get_property{};
    decltype(&mpv_wait_event) wait_event{};
    decltype(&mpv_render_context_create) render_context_create{};
    decltype(&mpv_render_context_set_update_callback) render_context_set_update_callback{};
    decltype(&mpv_render_context_get_info) render_context_get_info{};
    decltype(&mpv_render_context_update) render_context_update{};
    decltype(&mpv_render_context_render) render_context_render{};
    decltype(&mpv_render_context_free) render_context_free{};

    ~MpvApi() { if (module) FreeLibrary(module); }

    bool load() {
        std::vector<std::wstring> candidates;
        wchar_t explicit_path[32768]{};
        const DWORD n = GetEnvironmentVariableW(L"IKA_MPV_DLL", explicit_path, ARRAYSIZE(explicit_path));
        if (n && n < ARRAYSIZE(explicit_path)) candidates.emplace_back(explicit_path);
        wchar_t executable[32768]{};
        const DWORD length = GetModuleFileNameW(nullptr, executable, ARRAYSIZE(executable));
        if (length && length < ARRAYSIZE(executable)) {
            std::wstring beside(executable, length);
            const auto slash = beside.find_last_of(L"\\/");
            if (slash != std::wstring::npos) {
                beside.resize(slash + 1);
                candidates.push_back(beside + L"libmpv-2.dll");
                candidates.push_back(beside + L"mpv-2.dll");
            }
        }
        candidates.emplace_back(L"libmpv-2.dll");
        candidates.emplace_back(L"mpv-2.dll");
        for (const auto& candidate : candidates) {
            module = LoadLibraryExW(candidate.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
            if (module) break;
        }
        if (!module) return false;
#define MPV_LOAD(member, symbol) member = reinterpret_cast<decltype(member)>(GetProcAddress(module, symbol)); if (!member) return false
        MPV_LOAD(create, "mpv_create");
        MPV_LOAD(initialize, "mpv_initialize");
        MPV_LOAD(terminate_destroy, "mpv_terminate_destroy");
        MPV_LOAD(set_option_string, "mpv_set_option_string");
        MPV_LOAD(command, "mpv_command");
        MPV_LOAD(set_property_string, "mpv_set_property_string");
        MPV_LOAD(get_property, "mpv_get_property");
        MPV_LOAD(wait_event, "mpv_wait_event");
        MPV_LOAD(render_context_create, "mpv_render_context_create");
        MPV_LOAD(render_context_set_update_callback, "mpv_render_context_set_update_callback");
        MPV_LOAD(render_context_get_info, "mpv_render_context_get_info");
        MPV_LOAD(render_context_update, "mpv_render_context_update");
        MPV_LOAD(render_context_render, "mpv_render_context_render");
        MPV_LOAD(render_context_free, "mpv_render_context_free");
#undef MPV_LOAD
        return true;
    }
};

struct MpvPlayer {
    MpvApi api;
    mpv_handle* handle{};
    mpv_render_context* render{};
    ~MpvPlayer() {
        if (render) api.render_context_free(render);
        if (handle) api.terminate_destroy(handle);
    }
};

// Keep one copy of the first decoded frame. Near the loop point, blend it
// over the live frame in place; no extra decoder or intermediate render target.
struct LoopFade {
    ComPtr<ID3D11Texture2D> head;
    ComPtr<ID3D11ShaderResourceView> picture;
    ComPtr<ID3D11RenderTargetView> target;
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> ps;
    ComPtr<ID3D11BlendState> blend;
    D3D11_VIEWPORT viewport{};
    double duration{};

    HRESULT capture(ID3D11Device* device, ID3D11DeviceContext* context,
                    ID3D11Texture2D* shared, D3D11_TEXTURE2D_DESC desc) {
        viewport = {0.f, 0.f, static_cast<float>(desc.Width), static_cast<float>(desc.Height), 0.f, 1.f};
        desc.MiscFlags = 0;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        TRY(device->CreateTexture2D(&desc, nullptr, &head));
        context->CopyResource(head.Get(), shared);
        return S_OK;
    }

    HRESULT prepare(ID3D11Device* device, ID3D11Texture2D* shared, double seconds) {
        static constexpr char shader[] = R"(
            float4 vertex(uint i : SV_VertexID) : SV_Position {
                return float4(i == 1 ? 3.0 : -1.0, i == 2 ? -3.0 : 1.0, 0.0, 1.0);
            }
            Texture2D<float4> head : register(t0);
            float4 pixel(float4 p : SV_Position) : SV_Target {
                return head.Load(int3(int2(p.xy), 0));
            }
        )";
        ComPtr<ID3DBlob> vertex, pixel;
        TRY(D3DCompile(shader, sizeof(shader) - 1, nullptr, nullptr, nullptr,
                       "vertex", "vs_4_0", 0, 0, &vertex, nullptr));
        TRY(D3DCompile(shader, sizeof(shader) - 1, nullptr, nullptr, nullptr,
                       "pixel", "ps_4_0", 0, 0, &pixel, nullptr));
        TRY(device->CreateVertexShader(vertex->GetBufferPointer(), vertex->GetBufferSize(), nullptr, &vs));
        TRY(device->CreatePixelShader(pixel->GetBufferPointer(), pixel->GetBufferSize(), nullptr, &ps));
        D3D11_BLEND_DESC state{};
        auto& rt = state.RenderTarget[0];
        rt.BlendEnable = TRUE;
        rt.SrcBlend = D3D11_BLEND_BLEND_FACTOR;
        rt.DestBlend = D3D11_BLEND_INV_BLEND_FACTOR;
        rt.BlendOp = rt.BlendOpAlpha = D3D11_BLEND_OP_ADD;
        rt.SrcBlendAlpha = D3D11_BLEND_ONE;
        rt.DestBlendAlpha = D3D11_BLEND_ZERO;
        rt.RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_RED | D3D11_COLOR_WRITE_ENABLE_GREEN |
                                   D3D11_COLOR_WRITE_ENABLE_BLUE;
        TRY(device->CreateBlendState(&state, &blend));
        TRY(device->CreateRenderTargetView(shared, nullptr, &target));
        TRY(device->CreateShaderResourceView(head.Get(), nullptr, &picture));
        duration = seconds;
        return S_OK;
    }

    void apply(ID3D11DeviceContext* context, double position) const {
        if (duration == 0.0 || !std::isfinite(position)) return;
        const double span = (std::min)(0.65, duration * 0.12);
        const double left = duration - position;
        if (left >= span) return;
        const auto t = static_cast<float>(1.0 - (std::max)(0.0, left) / span);
        const auto w = t * t * (3.0f - 2.0f * t);
        const float factors[]{w, w, w, w};
        ID3D11RenderTargetView* rtv = target.Get();
        ID3D11ShaderResourceView* srv = picture.Get();
        context->IASetInputLayout(nullptr);
        context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context->VSSetShader(vs.Get(), nullptr, 0);
        context->GSSetShader(nullptr, nullptr, 0);
        context->PSSetShader(ps.Get(), nullptr, 0);
        context->PSSetShaderResources(0, 1, &srv);
        context->RSSetState(nullptr);
        context->RSSetViewports(1, &viewport);
        context->OMSetRenderTargets(1, &rtv, nullptr);
        context->OMSetBlendState(blend.Get(), factors, 0xffffffff);
        context->Draw(3, 0);
        context->OMSetRenderTargets(0, nullptr, nullptr);
        context->OMSetBlendState(nullptr, nullptr, 0xffffffff);
        srv = nullptr;
        context->PSSetShaderResources(0, 1, &srv);
    }
};

static HRESULT still(const wchar_t* file, ID3D11Device* device, ID3D11Device5* dev5,
                     ID3D11DeviceContext4* context, HANDLE parent) {
    D3D11_TEXTURE2D_DESC desc{};
    ComPtr<ID3D11Texture2D> shared;
    {
        ComPtr<IWICImagingFactory> factory;
        ComPtr<IWICBitmapDecoder> decoder;
        ComPtr<IWICBitmapFrameDecode> frame;
        ComPtr<IWICFormatConverter> convert;
        TRY(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)));
        TRY(factory->CreateDecoderFromFilename(file, nullptr, GENERIC_READ, WICDecodeMetadataCacheOnDemand, &decoder));
        TRY(decoder->GetFrame(0, &frame));
        TRY(frame->GetSize(&desc.Width, &desc.Height));
        if (!desc.Width || !desc.Height || desc.Width > 8192 || desc.Height > 8192) return E_INVALIDARG;
        TRY(factory->CreateFormatConverter(&convert));
        TRY(convert->Initialize(frame.Get(), GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone,
                                nullptr, 0.0, WICBitmapPaletteTypeCustom));
        const UINT stride = desc.Width * 4;
        const UINT size = stride * desc.Height;
        std::vector<BYTE> pixels(size);
        TRY(convert->CopyPixels(nullptr, stride, size, pixels.data()));
        desc.MipLevels = desc.ArraySize = desc.SampleDesc.Count = 1;
        desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
        desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED;
        const D3D11_SUBRESOURCE_DATA initial{pixels.data(), stride, size};
        TRY(device->CreateTexture2D(&desc, &initial, &shared));
    }
    ComPtr<IDXGIResource> resource;
    HANDLE texture{};
    TRY(shared.As(&resource));
    TRY(resource->GetSharedHandle(&texture));
    ComPtr<ID3D11Fence> ready, used;
    Handle ready_handle, used_handle;
    TRY(dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&ready)));
    TRY(dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&used)));
    TRY(ready->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &ready_handle.value));
    TRY(used->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &used_handle.value));
    TRY(context->Signal(ready.Get(), 1));
    context->Flush();
    cache_first_frame(context, shared.Get());
    if (!publish_info(desc, texture, ready_handle.value, used_handle.value)) return E_FAIL;
    SetProcessWorkingSetSize(GetCurrentProcess(), (SIZE_T)-1, (SIZE_T)-1);
    // The image is immutable; keep its handles alive without submitting fake frames.
    return WaitForSingleObject(parent, INFINITE) == WAIT_OBJECT_0 ? S_OK : E_FAIL;
}

static std::string utf8_path(const wchar_t* path) {
    const int count = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, path, -1,
                                          nullptr, 0, nullptr, nullptr);
    if (count <= 1) return {};
    std::string result(static_cast<size_t>(count), '\0');
    if (!WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, path, -1,
                             result.data(), count, nullptr, nullptr)) return {};
    result.resize(static_cast<size_t>(count - 1));
    return result;
}

static HRESULT mpv_failure(const wchar_t* message, int error = 0) {
    log_status(message, error < 0 ? E_FAIL : HRESULT_FROM_WIN32(ERROR_GEN_FAILURE));
    return E_FAIL;
}

// MPV's D3D11 render API lets its decoder write straight into the one shared
// texture consumed by the DX12 viewer. This removes Media Foundation's
// TransferVideoFrame compositor and its extra 3D pass while retaining the
// existing fence and first-frame contract.
static HRESULT play_mpv(const wchar_t* file, HANDLE parent) {
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> immediate;
    TRY(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT, nullptr, 0,
        D3D11_SDK_VERSION, &device, nullptr, &immediate));
    ComPtr<ID3D11Multithread> guard;
    TRY(device.As(&guard));
    guard->SetMultithreadProtected(TRUE);
    ComPtr<ID3D11Device5> dev5;
    ComPtr<ID3D11DeviceContext4> context;
    TRY(device.As(&dev5));
    TRY(immediate.As(&context));

    Handle update{CreateEventW(nullptr, FALSE, FALSE, nullptr)};
    Handle consumed{CreateEventW(nullptr, FALSE, FALSE, nullptr)};
    if (!update.value || !consumed.value) return HRESULT_FROM_WIN32(GetLastError());
    // Destroy libmpv before closing the event its render callback can signal.
    MpvPlayer player;
    if (!player.api.load()) return HRESULT_FROM_WIN32(ERROR_MOD_NOT_FOUND);
    player.handle = player.api.create();
    if (!player.handle) return mpv_failure(L"mpv_create failed");
    const auto option = [&](const char* name, const char* value) {
        return player.api.set_option_string(player.handle, name, value);
    };
    const bool copy_decode = GetEnvironmentVariableW(L"IKA_MPV_COPY", nullptr, 0) != 0;
    const bool software_decode = GetEnvironmentVariableW(L"IKA_MPV_SOFTWARE", nullptr, 0) != 0;
    // D3D11VA zero-copy and direct rendering are the key differences from the
    // old Media Foundation path. No OSD, scripts, subtitles or audio output
    // are needed for a wallpaper; the launcher controls mute separately.
    for (const auto [name, value] : std::initializer_list<std::pair<const char*, const char*>>{
        {"vo", "libmpv"}, {"profile", "fast"},
        {"hwdec", software_decode ? "no" : (copy_decode ? "d3d11va-copy" : "d3d11va")},
        {"d3d11va-zero-copy", software_decode || copy_decode ? "no" : "yes"},
        {"vd-lavc-dr", "yes"}, {"vd-lavc-threads", "2"}, {"hwdec-extra-frames", "2"}, {"loop-file", "inf"},
        {"keepaspect", "yes"},
        {"panscan", "1"}, {"osd-level", "0"}, {"osc", "no"},
        {"load-scripts", "no"}, {"audio-client-name", "Ika MPV"},
        // Never let mpv trade source frames for a lower GPU figure. Hardware
        // decode plus zero-copy presentation keeps the native cadence while
        // avoiding a second software/compositor copy.
        {"mute", "yes"}, {"volume", "0"},
        {"pause", "yes"},
        // Dropping decoded frames reduces GPU use but breaks source cadence.
        {"framedrop", "no"}, {"interpolation", "no"},
        {"video-timing-offset", "0"}, {"demuxer-max-bytes", "8MiB"},
        {"demuxer-max-back-bytes", "0MiB"}, {"cache", "no"},
        {"cache-secs", "0"}, {"video-reversal-buffer", "0"},
        {"audio-reversal-buffer", "0"},
        {"scale", "bilinear"}, {"cscale", "bilinear"}, {"dscale", "bilinear"},
        {"dither-depth", "no"}, {"correct-downscaling", "no"}, {"sigmoid-upscaling", "no"},
        {"hdr-compute-peak", "no"},
    }) {
        if (option(name, value) < 0) return mpv_failure(L"mpv option rejected");
    }
    char cover_start[48]{}, cover_end[48]{};
    if (GetEnvironmentVariableA("IKA_PROFILE_VIDEO_START", cover_start, sizeof(cover_start)) > 0 &&
        GetEnvironmentVariableA("IKA_PROFILE_VIDEO_END", cover_end, sizeof(cover_end)) > 0) {
        if (option("start", cover_start) < 0 || option("ab-loop-a", cover_start) < 0 ||
            option("ab-loop-b", cover_end) < 0 || option("audio", "no") < 0)
            return mpv_failure(L"mpv profile cover options rejected");
    }
    if (player.api.initialize(player.handle) < 0) return mpv_failure(L"mpv_initialize failed");
    mpv_d3d11_init_params init{device.Get()};
    const char* api_type = MPV_RENDER_API_TYPE_D3D11;
    mpv_render_param render_init[] = {
        {MPV_RENDER_PARAM_API_TYPE, const_cast<char*>(api_type)},
        {MPV_RENDER_PARAM_D3D11_INIT_PARAMS, &init},
        {MPV_RENDER_PARAM_INVALID, nullptr},
    };
    if (player.api.render_context_create(&player.render, player.handle, render_init) < 0)
        return mpv_failure(L"mpv D3D11 render context failed");
    player.api.render_context_set_update_callback(player.render, [](void* event) {
        SetEvent(static_cast<HANDLE>(event));
    }, update.value);
    const auto source = utf8_path(file);
    if (source.empty()) return E_INVALIDARG;
    const char* load[] = {"loadfile", source.c_str(), "replace", nullptr};
    if (player.api.command(player.handle, load) < 0) return mpv_failure(L"mpv loadfile failed");

    DWORD width = 0, height = 0;
    for (int attempt = 0; attempt != 10000 && (!width || !height); ++attempt) {
        const auto* event = player.api.wait_event(player.handle, 0.001);
        if (event && (event->event_id == MPV_EVENT_SHUTDOWN || event->event_id == MPV_EVENT_END_FILE))
            return mpv_failure(L"mpv ended before publishing a frame");
        int64_t w = 0, h = 0;
        if (player.api.get_property(player.handle, "video-params/w", MPV_FORMAT_INT64, &w) >= 0 &&
            player.api.get_property(player.handle, "video-params/h", MPV_FORMAT_INT64, &h) >= 0 &&
            w > 0 && h > 0 && w <= 8192 && h <= 8192) {
            width = static_cast<DWORD>(w);
            height = static_cast<DWORD>(h);
        }
    }
    if (!width || !height) return mpv_failure(L"mpv video dimensions unavailable");
    D3D11_TEXTURE2D_DESC desc{};
    desc.Width = width;
    desc.Height = height;
    desc.MipLevels = desc.ArraySize = desc.SampleDesc.Count = 1;
    desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
    desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED;
    ComPtr<ID3D11Texture2D> shared;
    TRY(device->CreateTexture2D(&desc, nullptr, &shared));
    ComPtr<IDXGIResource> resource;
    HANDLE texture_handle{};
    TRY(shared.As(&resource));
    TRY(resource->GetSharedHandle(&texture_handle));
    ComPtr<ID3D11Fence> ready, used;
    Handle ready_handle, used_handle;
    TRY(dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&ready)));
    TRY(dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&used)));
    TRY(ready->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &ready_handle.value));
    TRY(used->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &used_handle.value));

    const bool warm = GetEnvironmentVariableW(L"IKA_WE_BRIDGE_PREPARE", nullptr, 0) != 0;
    Handle audio{open_bridge_event(L"IKA_WE_BRIDGE_AUDIO", SYNCHRONIZE)};
    SceneAudio audio_session;
    audio_session.start(GetCurrentProcessId());
    bool muted = true;
    bool paused = true;
    LoopFade loop;
    double duration = 0.0;
    UINT64 serial = 0;
    UINT64 armed = 0;
    const bool loop_fade = GetEnvironmentVariableW(L"IKA_VIDEO_LOOP_FADE", nullptr, 0) != 0;
    const HANDLE waits_for_use[] = {parent, consumed.value};
    const HANDLE waits_for_mpv[] = {parent, update.value};

    while (WaitForSingleObject(parent, 0) == WAIT_TIMEOUT) {
        if (serial && used->GetCompletedValue() < serial) {
            if (armed != serial) {
                TRY(used->SetEventOnCompletion(serial, consumed.value));
                armed = serial;
            }
            // A hidden or retained preview eventually stops consuming. Pause
            // once, then sleep until the viewer actually uses that frame.
            const DWORD result = WaitForMultipleObjects(2, waits_for_use, FALSE, paused ? INFINITE : 80);
            if (result == WAIT_OBJECT_0) break;
            if (result == WAIT_FAILED) return HRESULT_FROM_WIN32(GetLastError());
            if (result == WAIT_TIMEOUT) {
                player.api.set_property_string(player.handle, "pause", "yes");
                paused = true;
            }
            continue;
        }
        if (paused) {
            player.api.set_property_string(player.handle, "pause", "no");
            paused = false;
        }
        const bool mute = !audio.value || WaitForSingleObject(audio.value, 0) != WAIT_OBJECT_0;
        if (mute != muted) {
            player.api.set_property_string(player.handle, "mute", mute ? "yes" : "no");
            player.api.set_property_string(player.handle, "volume", mute ? "0" : "100");
            audio_session.refresh();
            muted = mute;
        }
        const auto* event = player.api.wait_event(player.handle, 0.0);
        if (event && event->event_id == MPV_EVENT_SHUTDOWN) {
            return mpv_failure(L"mpv shutdown");
        }
        const auto updates = player.api.render_context_update(player.render);
        if (!(updates & MPV_RENDER_UPDATE_FRAME)) {
            // Let libmpv wake us as soon as the next frame can render.
            const DWORD result = WaitForMultipleObjects(2, waits_for_mpv, FALSE, 100);
            if (result == WAIT_OBJECT_0) break;
            if (result == WAIT_FAILED) return HRESULT_FROM_WIN32(GetLastError());
            continue;
        }
        mpv_render_frame_info frame_info{};
        mpv_render_param info_param{MPV_RENDER_PARAM_NEXT_FRAME_INFO, &frame_info};
        if (player.api.render_context_get_info(player.render, info_param) < 0 ||
            !(frame_info.flags & MPV_RENDER_FRAME_INFO_PRESENT)) {
            continue;
        }
        const bool repeat = (frame_info.flags & MPV_RENDER_FRAME_INFO_REPEAT) != 0;

        mpv_d3d11_fbo fbo{shared.Get()};
        int block = 1;
        int skip = repeat ? 1 : 0;
        mpv_render_param render_params[] = {
            {MPV_RENDER_PARAM_D3D11_FBO, &fbo},
            {MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME, &block},
            {MPV_RENDER_PARAM_SKIP_RENDERING, &skip},
            {MPV_RENDER_PARAM_INVALID, nullptr},
        };
        if (player.api.render_context_render(player.render, render_params) < 0) {
            return mpv_failure(L"mpv D3D11 frame render failed");
        }
        if (skip) continue;

        if (!serial) {
            double seconds = 0.0;
            if (player.api.get_property(player.handle, "duration", MPV_FORMAT_DOUBLE, &seconds) >= 0)
                duration = seconds;
            if (loop_fade && duration > 4.0 && FAILED(loop.capture(device.Get(), context.Get(), shared.Get(), desc)))
                loop = {};
        } else if (loop_fade && loop.duration > 0.0) {
            double position = 0.0;
            if (player.api.get_property(player.handle, "time-pos", MPV_FORMAT_DOUBLE, &position) >= 0)
                loop.apply(context.Get(), position);
        }
        TRY(context->Signal(ready.Get(), ++serial));
        context->Flush();
        if (serial == 1) {
            if (warm) {
                player.api.set_property_string(player.handle, "pause", "yes");
                paused = true;
            }
            cache_first_frame(context.Get(), shared.Get());
            if (!publish_info(desc, texture_handle, ready_handle.value, used_handle.value)) return E_FAIL;
            if (loop_fade && loop.head && FAILED(loop.prepare(device.Get(), shared.Get(), duration))) loop = {};
            SetProcessWorkingSetSize(GetCurrentProcess(), (SIZE_T)-1, (SIZE_T)-1);
        } else if (serial == 60) {
            SetProcessWorkingSetSize(GetCurrentProcess(), (SIZE_T)-1, (SIZE_T)-1);
        }
    }
    return S_OK;
}

static HRESULT play_media_foundation(const wchar_t* file, HANDLE parent, bool image) {
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> immediate;
    TRY(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT, nullptr, 0,
        D3D11_SDK_VERSION, &device, nullptr, &immediate));
    ComPtr<ID3D11Multithread> guard;
    TRY(device.As(&guard));
    guard->SetMultithreadProtected(TRUE);
    ComPtr<ID3D11Device5> dev5;
    ComPtr<ID3D11DeviceContext4> context;
    TRY(device.As(&dev5));
    TRY(immediate.As(&context));
    if (image) return still(file, device.Get(), dev5.Get(), context.Get(), parent);

    UINT token{};
    ComPtr<IMFDXGIDeviceManager> manager;
    TRY(MFCreateDXGIDeviceManager(&token, &manager));
    TRY(manager->ResetDevice(device.Get(), token));
    const auto events = Make<Events>();
    if (!events) return E_OUTOFMEMORY;
    ComPtr<IMFAttributes> attrs;
    TRY(MFCreateAttributes(&attrs, 3));
    TRY(attrs->SetUnknown(MF_MEDIA_ENGINE_CALLBACK, events.Get()));
    TRY(attrs->SetUnknown(MF_MEDIA_ENGINE_DXGI_MANAGER, manager.Get()));
    TRY(attrs->SetUINT32(MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM));
    ComPtr<IMFMediaEngineClassFactory> factory;
    TRY(CoCreateInstance(CLSID_MFMediaEngineClassFactory, nullptr, CLSCTX_INPROC_SERVER,
        IID_PPV_ARGS(&factory)));
    Player player;
    TRY(factory->CreateInstance(0, attrs.Get(), &player.engine));
    auto engine = player.engine.Get();
    TRY(engine->SetMuted(TRUE));
    TRY(engine->SetVolume(0.0));
    Handle audio{open_bridge_event(L"IKA_WE_BRIDGE_AUDIO", SYNCHRONIZE)};
    // Enforce the setting on the WASAPI session too: Media Foundation can
    // create a new audio renderer when the source or output device changes.
    SceneAudio audio_session;
    audio_session.start(GetCurrentProcessId());
    const bool warm = GetEnvironmentVariableW(L"IKA_WE_BRIDGE_PREPARE", nullptr, 0) != 0;
    bool muted = true;
    TRY(engine->SetLoop(TRUE));
    BSTR url = SysAllocString(file);
    if (!url) return E_OUTOFMEMORY;
    const auto loaded = engine->SetSource(url);
    SysFreeString(url);
    TRY(loaded);
    TRY(engine->SetMuted(TRUE));
    TRY(engine->SetVolume(0.0));
    TRY(engine->Play());

    ComPtr<ID3D11Texture2D> shared;
    ComPtr<ID3D11Fence> ready, used;
    Handle ready_handle, used_handle;
    HANDLE texture_handle{};
    UINT64 serial = 0;
    RECT output{};
    MFVideoNormalizedRect input{0, 0, 1, 1};
    MFARGB black{0, 0, 0, 255};
    LoopFade loop;
    double duration = 0.0;
    bool paused = false;
    const bool loop_fade = GetEnvironmentVariableW(L"IKA_VIDEO_LOOP_FADE", nullptr, 0) != 0;
    auto stall_start = std::chrono::steady_clock::now();
    while (WaitForSingleObject(parent, 4) == WAIT_TIMEOUT) {
        TRY(events->failure.load());
        const bool mute = !audio.value || WaitForSingleObject(audio.value, 0) != WAIT_OBJECT_0;
        if (mute != muted) {
            TRY(engine->SetVolume(mute ? 0.0 : 1.0));
            TRY(engine->SetMuted(mute));
            audio_session.refresh();
            muted = mute;
        }
        if (serial && used->GetCompletedValue() < serial) {
            if (!paused && std::chrono::steady_clock::now() - stall_start > std::chrono::milliseconds(120)) {
                TRY(engine->Pause());
                paused = true;
            }
            continue;
        }
        stall_start = std::chrono::steady_clock::now();
        if (paused) {
            TRY(engine->Play());
            paused = false;
        }
        LONGLONG timestamp{};
        const HRESULT tick = engine->OnVideoStreamTick(&timestamp);
        if (tick != S_OK) continue;
        D3D11_TEXTURE2D_DESC desc{};
        if (!shared) {
            DWORD width{}, height{};
            TRY(engine->GetNativeVideoSize(&width, &height));
            if (!width || !height || width > 8192 || height > 8192) return E_INVALIDARG;
            output = {0, 0, static_cast<LONG>(width), static_cast<LONG>(height)};
            desc.Width = width;
            desc.Height = height;
            desc.MipLevels = desc.ArraySize = desc.SampleDesc.Count = 1;
            desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
            desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
            desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED;
            TRY(device->CreateTexture2D(&desc, nullptr, &shared));
            ComPtr<IDXGIResource> resource;
            TRY(shared.As(&resource));
            TRY(resource->GetSharedHandle(&texture_handle));
            TRY(dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&ready)));
            TRY(dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&used)));
            TRY(ready->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &ready_handle.value));
            TRY(used->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &used_handle.value));
        }
        TRY(engine->TransferVideoFrame(shared.Get(), &input, &output, &black));
        if (!serial) {
            duration = engine->GetDuration();
            if (loop_fade && std::isfinite(duration) && duration > 4.0 &&
                FAILED(loop.capture(device.Get(), context.Get(), shared.Get(), desc))) {
                loop = {};
            }
        } else if (loop_fade && loop.duration > 0.0) {
            loop.apply(context.Get(), timestamp / 10'000'000.0);
        }
        TRY(context->Signal(ready.Get(), ++serial));
        context->Flush();
        if (serial == 1) {
            if (warm) {
                TRY(engine->Pause());
                paused = true;
            }
            cache_first_frame(context.Get(), shared.Get());
            if (!publish_info(desc, texture_handle, ready_handle.value, used_handle.value)) return E_FAIL;
            // Prepare the loop after publishing the first drawable frame.
            if (loop_fade && loop.head && FAILED(loop.prepare(device.Get(), shared.Get(), duration))) {
                loop = {};
            }
            SetProcessWorkingSetSize(GetCurrentProcess(), (SIZE_T)-1, (SIZE_T)-1);
        }
    }
    return S_OK;
}

int wmain(int argc, wchar_t** argv) {
    timeBeginPeriod(1);
    if (argc != 3 && (argc != 4 || wcscmp(argv[3], L"--image"))) return 2;
    wchar_t* end{};
    const auto pid = wcstoul(argv[2], &end, 10);
    Handle parent{(*end || !pid) ? nullptr : OpenProcess(SYNCHRONIZE, FALSE, pid)};
    if (!parent.value) return 3;
    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(hr)) return 4;
    // Images still use the tiny WIC path. Set IKA_VIDEO_BACKEND=mf only for
    // compatibility diagnostics; normal video playback is libmpv.
    wchar_t backend[32]{};
    const DWORD backend_size = GetEnvironmentVariableW(
        L"IKA_VIDEO_BACKEND", backend, ARRAYSIZE(backend));
    const bool use_media_foundation = argc == 4 ||
        (backend_size && backend_size < ARRAYSIZE(backend) && _wcsicmp(backend, L"mf") == 0);
    if (use_media_foundation) {
        if (SUCCEEDED(hr = MFStartup(MF_VERSION))) {
            hr = play_media_foundation(argv[1], parent.value, argc == 4);
            MFShutdown();
        }
    } else {
        hr = play_mpv(argv[1], parent.value);
        if (hr == HRESULT_FROM_WIN32(ERROR_MOD_NOT_FOUND)) {
            // Keep old installations usable until libmpv-2.dll is placed
            // beside video-mpv.exe; normal packaged runs never take this path.
            log_status(L"libmpv unavailable; using Media Foundation fallback", hr);
            if (SUCCEEDED(hr = MFStartup(MF_VERSION))) {
                hr = play_media_foundation(argv[1], parent.value, false);
                MFShutdown();
            }
        }
    }
    CoUninitialize();
    timeEndPeriod(1);
    if (FAILED(hr)) {
        log_status(L"Media playback failed", hr);
        return 5;
    }
    return 0;
}
