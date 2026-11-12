// ============================================================================
//  mkshortcut.exe —— 用 IShellLink 生成 Windows 快捷方式
//
//  用法：
//      mkshortcut.exe <目标exe> <工作目录> <快捷方式.lnk> [图标文件] [描述]
//
//  说明：
//      * 图标文件可以是 .ico，也可以直接指向 exe（exe 内嵌了图标资源）。
//      * 参数全部由调用方传入，程序内部不写死任何绝对路径，
//        换机器、换目录都不用改代码。
//      * 编译：
//          gcc -municode -O2 -o mkshortcut.exe mkshortcut.c -lole32 -luuid -lshell32
// ============================================================================

#define WIN32_LEAN_AND_MEAN
#define _WIN32_WINNT 0x0600

#include <windows.h>
#include <objbase.h>
#include <shobjidl.h>
#include <stdio.h>

static void usage(void)
{
    wprintf(L"用法: mkshortcut.exe <目标exe> <工作目录> <快捷方式.lnk> [图标] [描述]\n");
}

int wmain(int argc, wchar_t **argv)
{
    if (argc < 4) {
        usage();
        return 2;
    }

    const wchar_t *target = argv[1];
    const wchar_t *workdir = argv[2];
    const wchar_t *lnkfile = argv[3];
    const wchar_t *icon = (argc >= 5 && argv[4][0]) ? argv[4] : target;
    const wchar_t *desc = (argc >= 6) ? argv[5] : L"PureStat";

    HRESULT hr = CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
    if (FAILED(hr)) {
        wprintf(L"CoInitializeEx 失败: 0x%08lX\n", (unsigned long)hr);
        return 1;
    }

    IShellLinkW *link = NULL;
    hr = CoCreateInstance(&CLSID_ShellLink, NULL, CLSCTX_INPROC_SERVER,
                          &IID_IShellLinkW, (void **)&link);
    if (FAILED(hr) || !link) {
        wprintf(L"创建 IShellLink 失败: 0x%08lX\n", (unsigned long)hr);
        CoUninitialize();
        return 1;
    }

    link->lpVtbl->SetPath(link, target);
    link->lpVtbl->SetWorkingDirectory(link, workdir);
    link->lpVtbl->SetIconLocation(link, icon, 0);
    link->lpVtbl->SetDescription(link, desc);

    IPersistFile *pf = NULL;
    hr = link->lpVtbl->QueryInterface(link, &IID_IPersistFile, (void **)&pf);
    if (FAILED(hr) || !pf) {
        wprintf(L"取 IPersistFile 失败: 0x%08lX\n", (unsigned long)hr);
        link->lpVtbl->Release(link);
        CoUninitialize();
        return 1;
    }

    hr = pf->lpVtbl->Save(pf, lnkfile, TRUE);
    pf->lpVtbl->Release(pf);
    link->lpVtbl->Release(link);
    CoUninitialize();

    if (FAILED(hr)) {
        wprintf(L"保存快捷方式失败: 0x%08lX\n", (unsigned long)hr);
        return 1;
    }

    wprintf(L"已创建快捷方式: %ls\n", lnkfile);
    return 0;
}
