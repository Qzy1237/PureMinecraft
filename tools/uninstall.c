// ============================================================================
//  uninstall.exe —— PureMinecraft 专属卸载程序
//
//  用法：
//    uninstall.exe           只删程序本体，保留 .minecraft（存档/模组/版本）
//    uninstall.exe /purge    连 .minecraft 一起删（存档也会没）
// ============================================================================

#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS

#include <windows.h>
#include <shlobj.h>
#include <shlwapi.h>
#include <stdio.h>
#include <wchar.h>

#define APP_W L"PureMinecraft"

static const wchar_t *APP_KEY =
    L"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\" APP_W;

static void app_dir(wchar_t *out, DWORD n)
{
    GetModuleFileNameW(NULL, out, n);
    wchar_t *s = wcsrchr(out, L'\\');
    if (s) *s = 0;
}

int WINAPI wWinMain(HINSTANCE h, HINSTANCE p, PWSTR cmdline, int show)
{
    (void)h; (void)p; (void)show;

    int purge = (cmdline && wcsstr(cmdline, L"/purge") != NULL);

    wchar_t dir[MAX_PATH];
    app_dir(dir, MAX_PATH);

    // ---- 注册表 ----
    RegDeleteTreeW(HKEY_CURRENT_USER, APP_KEY);
    RegDeleteTreeW(HKEY_CURRENT_USER, L"Software\\Classes\\*\\shell\\PureMinecraft");
    RegDeleteTreeW(HKEY_CURRENT_USER, L"Software\\Classes\\Directory\\shell\\PureMinecraft");
    RegDeleteKeyValueW(HKEY_CURRENT_USER,
        L"Software\\Microsoft\\Windows\\CurrentVersion\\Run", APP_W);

    // ---- 快捷方式 ----
    wchar_t path[MAX_PATH];
    if (SUCCEEDED(SHGetFolderPathW(NULL, CSIDL_DESKTOPDIRECTORY, NULL, 0, path))) {
        PathAppendW(path, APP_W L".lnk");
        DeleteFileW(path);
    }
    if (SUCCEEDED(SHGetFolderPathW(NULL, CSIDL_PROGRAMS, NULL, 0, path))) {
        PathAppendW(path, APP_W);
        const wchar_t *names[] = { APP_W L".lnk", L"\u5378\u8f7d " APP_W L".lnk" };
        for (int i = 0; i < 2; ++i) {
            wchar_t f[MAX_PATH];
            wcscpy(f, path);
            PathAppendW(f, names[i]);
            DeleteFileW(f);
        }
        RemoveDirectoryW(path);
    }

    // ---- 组装清理命令 ----
    // 只删程序自己的东西；.minecraft 只有 /purge 才动
    static const wchar_t *PROG_DIRS[] = {
        L"bin", L"config", L"resources", L"src", L"src-tauri",
        L"tools", L"stage", L"dist", L"java"
    };

    wchar_t sys[MAX_PATH];
    GetSystemDirectoryW(sys, MAX_PATH);

    wchar_t cmdstr[8192];
    int n = _snwprintf(cmdstr, 8192,
        L"\"%s\\cmd.exe\" /c ping 127.0.0.1 -n 3 >nul", sys);

    for (int i = 0; i < (int)(sizeof(PROG_DIRS) / sizeof(PROG_DIRS[0])); ++i) {
        n += _snwprintf(cmdstr + n, 8192 - n,
            L" & rmdir /s /q \"%s\\%s\" 2>nul", dir, PROG_DIRS[i]);
    }

    // 根目录下的散文件（README / 截图 / 日志）
    n += _snwprintf(cmdstr + n, 8192 - n,
        L" & del /q \"%s\\*.txt\" \"%s\\*.png\" \"%s\\*.log\" 2>nul", dir, dir, dir);

    if (purge) {
        n += _snwprintf(cmdstr + n, 8192 - n,
            L" & rmdir /s /q \"%s\\.minecraft\" 2>nul", dir);
    }

    // 自删 + 删空目录（非递归，只有真空了才删得掉）
    n += _snwprintf(cmdstr + n, 8192 - n,
        L" & del /q \"%s\\uninstall.exe\" 2>nul & rmdir \"%s\" 2>nul", dir, dir);

    cmdstr[8191] = 0;

    STARTUPINFOW si;
    PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = SW_HIDE;
    ZeroMemory(&pi, sizeof(pi));

    if (CreateProcessW(NULL, cmdstr, NULL, NULL, FALSE,
                       CREATE_NO_WINDOW, NULL, NULL, &si, &pi)) {
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
    }
    return 0;
}
