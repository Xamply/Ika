#include <windows.h>
#include <detours.h>
#include <algorithm>
#include <cwchar>
#include <string>
#include <cstdio>
#include "audio_sessions.h"

static std::wstring quote(const wchar_t* arg) { return std::wstring(L"\"") + arg + L"\""; }

struct Handle {
    HANDLE value{};
    ~Handle() { if (value) CloseHandle(value); }
};

int wmain(int argc, wchar_t** argv) {
    if (argc != 4 && (argc != 5 || wcscmp(argv[4], L"--scene-720p") != 0)) return 2;
    Handle parent;
    wchar_t parent_arg[32]{};
    const DWORD parent_len = GetEnvironmentVariableW(L"IKA_WE_BRIDGE_PARENT", parent_arg, ARRAYSIZE(parent_arg));
    if (parent_len) {
        wchar_t* end{};
        if (parent_len >= ARRAYSIZE(parent_arg) || parent_arg[0] < L'0' || parent_arg[0] > L'9') return 2;
        const auto id = wcstoull(parent_arg, &end, 10);
        if (!id || *end) return 2;
        parent.value = reinterpret_cast<HANDLE>(id);
        if (WaitForSingleObject(parent.value, 0) != WAIT_TIMEOUT) return 2;
        // The engine must not inherit the host's contract with Python.
        SetEnvironmentVariableW(L"IKA_WE_BRIDGE_PARENT", nullptr);
    }
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    DEVMODEW mode{};
    mode.dmSize = sizeof(mode);
    if (!EnumDisplaySettingsW(nullptr, ENUM_CURRENT_SETTINGS, &mode) ||
        !mode.dmPelsWidth || !mode.dmPelsHeight ||
        mode.dmPelsWidth > 8192 || mode.dmPelsHeight > 8192) {
        fprintf(stderr, "Unsupported primary monitor size\n");
        return 2;
    }
    int width = static_cast<int>(mode.dmPelsWidth);
    int height = static_cast<int>(mode.dmPelsHeight);
    // Limit the source, retaining the monitor's aspect ratio. The viewer
    // and its UI still render at the physical window size.
    if (argc == 5) {
        const int long_side = (std::max)(width, height);
        const int short_side = (std::min)(width, height);
        const double scale = (std::min)({1.0, 1280.0 / long_side, 720.0 / short_side});
        width = (std::max)(1, static_cast<int>(width * scale + 0.5));
        height = (std::max)(1, static_cast<int>(height * scale + 0.5));
    }
    const auto engine = argv[1];
    auto source = CreateWindowExW(WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, L"STATIC", L"Ika isolated texture host",
        WS_POPUP | WS_VISIBLE, GetSystemMetrics(SM_XVIRTUALSCREEN) - width - 128,
        GetSystemMetrics(SM_YVIRTUALSCREEN) - height - 128, width, height, nullptr, nullptr, nullptr, nullptr);
    if (!source) return 3;
    auto job = CreateJobObjectW(nullptr, nullptr);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limit{};
    limit.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (!job || !SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limit, sizeof(limit))) return 4;
    std::wstring command = quote(engine) + L" -screensaver -host " +
        std::to_wstring(reinterpret_cast<UINT_PTR>(source)) + L" -preview";
    // Detours takes the DLL path in the current ANSI code page.
    char dll[32768]{};
    BOOL inexact = FALSE;
    if (!WideCharToMultiByte(CP_ACP, 0, argv[2], -1, dll, sizeof(dll), nullptr, &inexact) || inexact) return 5;
    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    PROCESS_INFORMATION child{};
    if (!DetourCreateProcessWithDllExW(engine, command.data(), nullptr, nullptr, FALSE,
        CREATE_SUSPENDED | CREATE_NO_WINDOW, nullptr, argv[3], &startup, &child, dll, nullptr)) {
        fprintf(stderr, "Isolated engine could not start: %lu\n", GetLastError());
        return 6;
    }
    if (!AssignProcessToJobObject(job, child.hProcess)) {
        TerminateProcess(child.hProcess, 1);
        fprintf(stderr, "Could not contain the isolated worker: %lu\n", GetLastError());
        return 7;
    }
    SceneAudio audio;
    // Arm session muting before the suspended scene can create its audio output.
    audio.start(child.dwProcessId);
    ResumeThread(child.hThread);
    CloseHandle(child.hThread);
    HANDLE wait[]{child.hProcess, nullptr, nullptr};
    DWORD count = 1;
    const DWORD parent_index = parent.value ? count : MAXDWORD;
    if (parent.value) wait[count++] = parent.value;
    const DWORD audio_index = audio.event() ? count : MAXDWORD;
    if (audio.event()) wait[count++] = audio.event();
    bool owner_exited = false;
    for (;;) {
        const DWORD event = MsgWaitForMultipleObjects(count, wait, FALSE, INFINITE, QS_ALLINPUT);
        if (parent.value && event == WAIT_OBJECT_0 + parent_index) {
            owner_exited = true;
            break;
        }
        if (audio.event() && event == WAIT_OBJECT_0 + audio_index) {
            audio.refresh();
            continue;
        }
        if (event != WAIT_OBJECT_0 + count) break;
        MSG msg;
        while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    DWORD code{};
    GetExitCodeProcess(child.hProcess, &code);
    CloseHandle(child.hProcess);
    CloseHandle(job);
    return owner_exited ? 0 : static_cast<int>(code);
}
