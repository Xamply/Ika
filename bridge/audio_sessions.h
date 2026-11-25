#pragma once
#include <windows.h>
#include <audiopolicy.h>
#include <mmdeviceapi.h>
#include <wrl.h>
#include <mutex>
#include <vector>
#include "frame_info.h"

// Only control the isolated scene's own sessions. The installed Wallpaper
// Engine and other applications are never changed.
class SceneSessions final : public Microsoft::WRL::RuntimeClass<
    Microsoft::WRL::RuntimeClassFlags<Microsoft::WRL::ClassicCom>, IAudioSessionNotification> {
    std::mutex lock;
    std::vector<Microsoft::WRL::ComPtr<ISimpleAudioVolume>> volumes;
    DWORD pid{};
    bool muted = true;

public:
    void target(DWORD child) { pid = child; }

    void mute(bool silence) {
        std::lock_guard guard(lock);
        muted = silence;
        for (const auto& session : volumes) session->SetMute(muted, nullptr);
    }

    IFACEMETHODIMP OnSessionCreated(IAudioSessionControl* control) override {
        Microsoft::WRL::ComPtr<IAudioSessionControl2> session;
        DWORD process{};
        if (FAILED(control->QueryInterface(IID_PPV_ARGS(&session))) ||
            FAILED(session->GetProcessId(&process)) || process != pid) return S_OK;
        Microsoft::WRL::ComPtr<ISimpleAudioVolume> volume;
        if (SUCCEEDED(session.As(&volume))) {
            std::lock_guard guard(lock);
            volume->SetMute(muted, nullptr);
            volumes.push_back(std::move(volume));
        }
        return S_OK;
    }
};

class SceneAudio {
    HANDLE enabled = open_bridge_event(L"IKA_WE_BRIDGE_AUDIO", SYNCHRONIZE);
    HANDLE changed = open_bridge_event(L"IKA_WE_BRIDGE_AUDIO_WAKE", SYNCHRONIZE);
    Microsoft::WRL::ComPtr<IAudioSessionManager2> manager;
    Microsoft::WRL::ComPtr<SceneSessions> sessions;
    bool apartment = false;
    bool registered = false;

public:
    HANDLE event() const { return changed; }

    void start(DWORD pid) {
        if (!enabled || FAILED(CoInitializeEx(nullptr, COINIT_MULTITHREADED))) return;
        apartment = true;
        Microsoft::WRL::ComPtr<IMMDeviceEnumerator> enumerator;
        Microsoft::WRL::ComPtr<IMMDevice> endpoint;
        if (FAILED(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_INPROC_SERVER,
                                    IID_PPV_ARGS(&enumerator))) ||
            FAILED(enumerator->GetDefaultAudioEndpoint(eRender, eMultimedia, &endpoint)) ||
            FAILED(endpoint->Activate(__uuidof(IAudioSessionManager2), CLSCTX_ALL, nullptr,
                                      reinterpret_cast<void**>(manager.GetAddressOf())))) return;
        sessions = Microsoft::WRL::Make<SceneSessions>();
        if (!sessions) return;
        sessions->target(pid);
        registered = SUCCEEDED(manager->RegisterSessionNotification(sessions.Get()));
        if (!registered) return;
        // WASAPI starts delivering new sessions after the first GetCount.
        Microsoft::WRL::ComPtr<IAudioSessionEnumerator> existing;
        int count{};
        if (SUCCEEDED(manager->GetSessionEnumerator(&existing)) &&
            SUCCEEDED(existing->GetCount(&count))) {
            for (int i = 0; i < count; ++i) {
                Microsoft::WRL::ComPtr<IAudioSessionControl> control;
                if (SUCCEEDED(existing->GetSession(i, &control))) sessions->OnSessionCreated(control.Get());
            }
        }
        refresh();
    }

    void refresh() {
        if (registered) sessions->mute(WaitForSingleObject(enabled, 0) != WAIT_OBJECT_0);
    }

    ~SceneAudio() {
        if (registered) manager->UnregisterSessionNotification(sessions.Get());
        sessions.Reset();
        manager.Reset();
        if (apartment) CoUninitialize();
        if (changed) CloseHandle(changed);
        if (enabled) CloseHandle(enabled);
    }
};
