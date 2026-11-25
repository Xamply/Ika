#pragma once
#include <windows.h>
#include <d3d11.h>
#include <cstdio>
#include <cwchar>

inline HANDLE open_bridge_event(const wchar_t* key, DWORD access) {
    wchar_t name[256]{};
    const DWORD size = GetEnvironmentVariableW(key, name, ARRAYSIZE(name));
    return size && size < ARRAYSIZE(name) ? OpenEventW(access, FALSE, name) : nullptr;
}

inline void log_status(const wchar_t* message, HRESULT hr) {
    wchar_t file[32768]{};
    DWORD n = GetEnvironmentVariableW(L"IKA_WE_BRIDGE_LOG", file, ARRAYSIZE(file));
    if (!n || n >= ARRAYSIZE(file)) return;
    FILE* fp{};
    if (!_wfopen_s(&fp, file, L"a")) { fwprintf(fp, L"%ls: 0x%08lx\n", message, hr); fclose(fp); }
}

inline bool publish_info(const D3D11_TEXTURE2D_DESC& desc, HANDLE texture, HANDLE ready, HANDLE used) {
    wchar_t file[32768]{};
    DWORD n = GetEnvironmentVariableW(L"IKA_WE_BRIDGE_INFO", file, ARRAYSIZE(file));
    if (!n || n + 5 >= ARRAYSIZE(file)) return false;
    wchar_t tmp[32768]{};
    swprintf_s(tmp, L"%ls.tmp", file);
    FILE* fp{};
    if (_wfopen_s(&fp, tmp, L"w")) return false;
    fprintf(fp, "%lu %u %u %u %llu %llu %llu\n", GetCurrentProcessId(), desc.Width, desc.Height,
            static_cast<unsigned>(desc.Format), reinterpret_cast<unsigned long long>(texture),
            reinterpret_cast<unsigned long long>(ready),
            reinterpret_cast<unsigned long long>(used));
    if (fclose(fp)) return false;
    if (!MoveFileExW(tmp, file, MOVEFILE_REPLACE_EXISTING)) return false;
    if (const auto ready_event = open_bridge_event(L"IKA_WE_BRIDGE_FRAME_EVENT", EVENT_MODIFY_STATE)) {
        SetEvent(ready_event);
        CloseHandle(ready_event);
    }
    return true;
}
