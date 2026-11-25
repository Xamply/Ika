#include <windows.h>
#include <d3d11_4.h>
#include <dxgi1_2.h>
#include <detours.h>
#include <wrl/client.h>
#include "frame_info.h"
#include "frame_cache.h"
#include <mutex>

using Microsoft::WRL::ComPtr;
extern "C" void DetourOrdinal1() {}
using Present = HRESULT(STDMETHODCALLTYPE*)(IDXGISwapChain*, UINT, UINT);
using Present1 = HRESULT(STDMETHODCALLTYPE*)(IDXGISwapChain1*, UINT, UINT, const DXGI_PRESENT_PARAMETERS*);
using GetBuffer = HRESULT(STDMETHODCALLTYPE*)(IDXGISwapChain*, UINT, REFIID, void**);
static Present original;
static Present1 original1;
static GetBuffer original_getbuffer;
static std::recursive_mutex bridge_lock;

struct Bridge {
    ComPtr<ID3D11Texture2D> shared;
    ComPtr<ID3D11DeviceContext4> context;
    ComPtr<ID3D11Fence> ready, used;
    HANDLE ready_handle{}, used_handle{};
    HANDLE texture{};
    HANDLE consumed{};
    UINT64 serial = 0;
    IDXGISwapChain* source{};
    bool direct = false;
    bool alias = false;
    bool nt = false;
    bool read_only = false;
    bool flush = true;
    bool region = false;
    ~Bridge() {
        if (consumed) CloseHandle(consumed);
        if (texture) CloseHandle(texture);
    }
};
static Bridge bridge;

enum class TransferArch {
    Copy, CopyNt, CopyRead, CopyRegion, CopyRegionRead, CopyNoFlush, Direct, DirectAlias
};

static TransferArch transfer_arch() {
    wchar_t value[32]{};
    const DWORD size = GetEnvironmentVariableW(L"IKA_TRANSFER_ARCH", value, ARRAYSIZE(value));
    if (size && size < ARRAYSIZE(value)) {
        if (_wcsicmp(value, L"copy") == 0) return TransferArch::Copy;
        if (_wcsicmp(value, L"direct") == 0) return TransferArch::Direct;
        if (_wcsicmp(value, L"copy_nt") == 0) return TransferArch::CopyNt;
        if (_wcsicmp(value, L"copy_read") == 0) return TransferArch::CopyRead;
        if (_wcsicmp(value, L"copy_region") == 0) return TransferArch::CopyRegion;
        if (_wcsicmp(value, L"copy_region_read") == 0) return TransferArch::CopyRegionRead;
        if (_wcsicmp(value, L"copy_noflush") == 0) return TransferArch::CopyNoFlush;
        if (_wcsicmp(value, L"direct_alias") == 0) return TransferArch::DirectAlias;
    }
    // DirectAlias is production default: the engine renders into the shared
    // target, so bridge performs no full-frame GPU copy.
    return TransferArch::DirectAlias;
}

static HRESULT init(IDXGISwapChain* swap, ID3D11Texture2D* back) {
    ComPtr<ID3D11Device> dev;
    ComPtr<ID3D11Device5> dev5;
    ComPtr<ID3D11DeviceContext> immediate;
    back->GetDevice(&dev);
    auto hr = dev.As(&dev5);
    if (FAILED(hr)) return hr;
    dev->GetImmediateContext(&immediate);
    if (FAILED(hr = immediate.As(&bridge.context))) return hr;
    D3D11_TEXTURE2D_DESC desc{};
    back->GetDesc(&desc);
    if (desc.SampleDesc.Count != 1 || (desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM &&
        desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM)) return DXGI_ERROR_UNSUPPORTED;
    const auto arch = transfer_arch();
    bridge.direct = arch == TransferArch::Direct;
    bridge.alias = arch == TransferArch::DirectAlias;
    bridge.nt = arch == TransferArch::CopyNt;
    bridge.read_only = arch == TransferArch::CopyRead || arch == TransferArch::CopyRegionRead;
    bridge.flush = arch != TransferArch::CopyNoFlush;
    bridge.region = arch == TransferArch::CopyRegion || arch == TransferArch::CopyRegionRead;
    if (bridge.direct && !bridge.alias) {
        // A swapchain backbuffer may already be shareable. Reusing its handle
        // removes the producer-side CopyResource. If the engine created a
        // private backbuffer, caller falls back to the copy architecture.
        ComPtr<IDXGIResource> dxgi;
        HRESULT direct_hr = back->QueryInterface(IID_PPV_ARGS(&dxgi));
        if (SUCCEEDED(direct_hr)) direct_hr = dxgi->GetSharedHandle(&bridge.texture);
        if (FAILED(direct_hr) || !bridge.texture) {
            // Some drivers expose only the NT-handle path. It is still the
            // same backbuffer and avoids allocating a second full-size image.
            ComPtr<IDXGIResource1> resource1;
            bridge.texture = nullptr;
            direct_hr = back->QueryInterface(IID_PPV_ARGS(&resource1));
            if (SUCCEEDED(direct_hr)) {
                direct_hr = resource1->CreateSharedHandle(
                    nullptr, DXGI_SHARED_RESOURCE_READ, nullptr, &bridge.texture);
                if (FAILED(direct_hr)) {
                    bridge.texture = nullptr;
                    direct_hr = resource1->CreateSharedHandle(
                        nullptr, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE,
                        nullptr, &bridge.texture);
                }
            }
        }
        if (SUCCEEDED(direct_hr) && bridge.texture) {
            log_status(L"Direct frame bridge selected", S_OK);
        } else {
            log_status(L"Direct frame bridge unavailable", direct_hr);
            // Keep the experiment isolated: an explicit direct request must
            // never silently benchmark the copy path under a different name.
            return direct_hr;
        }
    }
    if (!bridge.direct || bridge.alias) {
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE |
            (bridge.read_only ? 0u : D3D11_BIND_RENDER_TARGET);
        desc.CPUAccessFlags = 0;
        desc.MiscFlags = bridge.nt ? D3D11_RESOURCE_MISC_SHARED_NTHANDLE : D3D11_RESOURCE_MISC_SHARED;
        if (FAILED(hr = dev->CreateTexture2D(&desc, nullptr, &bridge.shared))) {
            log_status(L"Copy texture creation failed", hr);
            return hr;
        }
        if (bridge.nt) {
            ComPtr<IDXGIResource1> dxgi;
            if (FAILED(hr = bridge.shared.As(&dxgi))) {
                log_status(L"Copy NT resource query failed", hr);
                return hr;
            }
            if (FAILED(hr = dxgi->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr,
                                                      &bridge.texture))) {
                log_status(L"Copy NT handle creation failed", hr);
                return hr;
            }
            log_status(L"Copy NT frame bridge selected", S_OK);
        } else {
            ComPtr<IDXGIResource> dxgi;
            if (FAILED(hr = bridge.shared.As(&dxgi))) {
                log_status(L"Copy resource query failed", hr);
                return hr;
            }
            if (FAILED(hr = dxgi->GetSharedHandle(&bridge.texture))) {
                log_status(L"Copy handle creation failed", hr);
                return hr;
            }
            log_status(bridge.alias ? L"Direct alias target selected" :
                                     bridge.read_only ? L"Copy read-only bridge selected" :
                                                         L"Copy frame bridge selected", S_OK);
        }
    }
    if (FAILED(hr = dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&bridge.ready)))) return hr;
    if (FAILED(hr = dev5->CreateFence(0, D3D11_FENCE_FLAG_SHARED, IID_PPV_ARGS(&bridge.used)))) return hr;
    if (FAILED(hr = bridge.ready->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &bridge.ready_handle))) return hr;
    if (FAILED(hr = bridge.used->CreateSharedHandle(nullptr, GENERIC_ALL, nullptr, &bridge.used_handle))) return hr;
    bridge.consumed = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!bridge.consumed) return HRESULT_FROM_WIN32(GetLastError());
    bridge.source = swap;
    log_status(L"Official frame bridge ready", S_OK);
    return S_OK;
}

static bool wallpaper_swapchain(IDXGISwapChain* swap) {
    DXGI_SWAP_CHAIN_DESC desc{};
    if (FAILED(swap->GetDesc(&desc)) || !desc.OutputWindow) return false;
    wchar_t cls[128]{};
    return GetClassNameW(desc.OutputWindow, cls, ARRAYSIZE(cls)) &&
           wcscmp(cls, L"WPEDesktopDX11Window") == 0;
}

static HRESULT STDMETHODCALLTYPE getbuffer(IDXGISwapChain* swap, UINT index, REFIID riid,
                                            void** surface) {
    const auto hr = original_getbuffer(swap, index, riid, surface);
    if (FAILED(hr) || index != 0 || riid != __uuidof(ID3D11Texture2D) ||
        transfer_arch() != TransferArch::DirectAlias || !wallpaper_swapchain(swap)) {
        return hr;
    }
    std::lock_guard lock(bridge_lock);
    if (!bridge.source) {
        ComPtr<ID3D11Texture2D> original_back;
        if (FAILED(original_getbuffer(swap, index, IID_PPV_ARGS(&original_back)))) return hr;
        const auto init_hr = init(swap, original_back.Get());
        if (FAILED(init_hr)) {
            log_status(L"Direct alias initialization failed", init_hr);
            return init_hr;
        }
        log_status(L"Direct alias bridge selected", S_OK);
    }
    if (bridge.alias && bridge.source == swap) {
        // Engine receives the shared texture as its render target. Present is
        // still intercepted, but no frame copy is needed afterward.
        if (*surface) {
            reinterpret_cast<IUnknown*>(*surface)->Release();
            *surface = nullptr;
        }
        return bridge.shared->QueryInterface(riid, surface);
    }
    return hr;
}

static bool intercept(IDXGISwapChain* swap, UINT flags) {
    if (flags & DXGI_PRESENT_TEST) return false;
    HWND hwnd{};
    DXGI_SWAP_CHAIN_DESC desc{};
    if (SUCCEEDED(swap->GetDesc(&desc))) hwnd = desc.OutputWindow;
    wchar_t cls[128]{};
    if (!hwnd || !GetClassNameW(hwnd, cls, ARRAYSIZE(cls)) || wcscmp(cls, L"WPEDesktopDX11Window"))
        return false;
    std::lock_guard lock(bridge_lock);
    if (bridge.source && bridge.source != swap) return false;
    // Scene pacing and the consumption fence below already provide backpressure.
    if (bridge.serial && bridge.used->GetCompletedValue() < bridge.serial) return true;
    ComPtr<ID3D11Texture2D> back;
    if (FAILED(swap->GetBuffer(0, IID_PPV_ARGS(&back)))) return false;
    static bool failed = false;
    if (failed) return false;
    if (!bridge.source) {
        auto hr = init(swap, back.Get());
        if (FAILED(hr)) { failed = true; log_status(L"Share initialization failed", hr); return false; }
    }
    if (!bridge.direct && !bridge.alias) {
        if (bridge.region) {
            D3D11_TEXTURE2D_DESC region_desc{};
            back->GetDesc(&region_desc);
            D3D11_BOX box{0, 0, 0, region_desc.Width, region_desc.Height, 1};
            bridge.context->CopySubresourceRegion(bridge.shared.Get(), 0, 0, 0, 0,
                                                   back.Get(), 0, &box);
        } else {
            bridge.context->CopyResource(bridge.shared.Get(), back.Get());
        }
    }
    const auto hr = bridge.context->Signal(bridge.ready.Get(), ++bridge.serial);
    if (bridge.flush) bridge.context->Flush();
    if (FAILED(hr)) { log_status(L"Signal failed", hr); return false; }
    if (bridge.serial == 1) {
        D3D11_TEXTURE2D_DESC shared_desc{};
        (bridge.direct ? back.Get() : bridge.shared.Get())->GetDesc(&shared_desc);
        cache_first_frame(bridge.context.Get(), bridge.direct ? back.Get() : bridge.shared.Get());
        if (!publish_info(shared_desc, bridge.texture, bridge.ready_handle, bridge.used_handle)) return false;
    }
    // Stop building frames when the viewer has stopped consuming them.
    // The isolated host closes its job when the producer is no longer needed.
    const auto wait = bridge.used->SetEventOnCompletion(bridge.serial, bridge.consumed);
    if (FAILED(wait)) { log_status(L"Frame consumption wait failed", wait); return false; }
    if (WaitForSingleObject(bridge.consumed, INFINITE) != WAIT_OBJECT_0) return false;
    return true;
}

static HRESULT STDMETHODCALLTYPE present(IDXGISwapChain* s, UINT sync, UINT flags) {
    return intercept(s, flags) ? S_OK : original(s, sync, flags);
}
static HRESULT STDMETHODCALLTYPE present1(IDXGISwapChain1* s, UINT sync, UINT flags, const DXGI_PRESENT_PARAMETERS* p) {
    return intercept(s, flags) ? S_OK : original1(s, sync, flags, p);
}

static DWORD WINAPI install(void*) {
    HWND hwnd = CreateWindowExW(0, L"STATIC", L"", WS_POPUP, 0, 0, 8, 8, nullptr, nullptr, nullptr, nullptr);
    if (!hwnd) return 1;
    DXGI_SWAP_CHAIN_DESC desc{};
    desc.BufferDesc.Width = desc.BufferDesc.Height = 8;
    desc.BufferDesc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.BufferCount = 2;
    desc.OutputWindow = hwnd;
    desc.Windowed = TRUE;
    desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
    ComPtr<IDXGISwapChain> sample;
    const auto hr = D3D11CreateDeviceAndSwapChain(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, &desc, &sample, nullptr, nullptr, nullptr);
    if (FAILED(hr)) { DestroyWindow(hwnd); log_status(L"Probe device failed", hr); return 2; }
    ComPtr<IDXGISwapChain1> sample1;
    sample.As(&sample1);
    original = reinterpret_cast<Present>((*reinterpret_cast<void***>(sample.Get()))[8]);
    original_getbuffer = reinterpret_cast<GetBuffer>((*reinterpret_cast<void***>(sample.Get()))[9]);
    if (sample1) original1 = reinterpret_cast<Present1>((*reinterpret_cast<void***>(sample1.Get()))[22]);
    DetourTransactionBegin();
    DetourUpdateThread(GetCurrentThread());
    DetourAttach(&(PVOID&)original, present);
    DetourAttach(&(PVOID&)original_getbuffer, getbuffer);
    if (original1) DetourAttach(&(PVOID&)original1, present1);
    auto result = DetourTransactionCommit();
    log_status(L"DXGI boundary installed", HRESULT_FROM_WIN32(result));
    sample1.Reset(); sample.Reset(); DestroyWindow(hwnd);
    return result;
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (DetourIsHelperProcess()) return TRUE;
    if (reason == DLL_PROCESS_ATTACH) {
        DetourRestoreAfterWith();
        DisableThreadLibraryCalls(instance);
        if (auto thread = CreateThread(nullptr, 0, install, nullptr, 0, nullptr)) CloseHandle(thread);
    }
    return TRUE;
}
