#pragma once
#include <windows.h>
#include <compressapi.h>
#include <d3d11.h>
#include <wrl/client.h>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

// Write a lossless copy of the first real frame, with no color conversion.
// The viewer accepts only complete files; the rename is the commit point.
inline void cache_first_frame(ID3D11DeviceContext* context, ID3D11Texture2D* source) {
    wchar_t path[32768]{};
    const DWORD n = GetEnvironmentVariableW(L"IKA_WE_BRIDGE_CACHE_FRAME", path, ARRAYSIZE(path));
    if (!n || n >= ARRAYSIZE(path)) return;
    try {
        using Microsoft::WRL::ComPtr;
        D3D11_TEXTURE2D_DESC desc{};
        source->GetDesc(&desc);
        if (!desc.Width || !desc.Height || desc.Width > 8192 || desc.Height > 8192 ||
            desc.SampleDesc.Count != 1 || (desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM &&
            desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM)) return;
        const UINT row = desc.Width * 4;
        const SIZE_T size = static_cast<SIZE_T>(row) * desc.Height;
        const UINT header[]{0x31464b49u, desc.Width, desc.Height, static_cast<UINT>(desc.Format)};
        std::vector<unsigned char> pixels(size);
        {
            ComPtr<ID3D11Device> device;
            ComPtr<ID3D11Texture2D> staging;
            context->GetDevice(&device);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
            desc.BindFlags = desc.MiscFlags = 0;
            if (FAILED(device->CreateTexture2D(&desc, nullptr, &staging))) return;
            context->CopyResource(staging.Get(), source);
            D3D11_MAPPED_SUBRESOURCE map{};
            if (FAILED(context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &map))) return;
            for (UINT y = 0; y < desc.Height; ++y) {
                std::memcpy(pixels.data() + static_cast<SIZE_T>(y) * row,
                    static_cast<const unsigned char*>(map.pData) + static_cast<SIZE_T>(y) * map.RowPitch, row);
            }
            context->Unmap(staging.Get(), 0);
        }
        std::vector<unsigned char> compressed(size);
        SIZE_T packed = 0;
        COMPRESSOR_HANDLE encoder{};
        if (CreateCompressor(COMPRESS_ALGORITHM_XPRESS, nullptr, &encoder)) {
            if (!Compress(encoder, pixels.data(), size, compressed.data(), size, &packed) || packed >= size)
                packed = 0;
            CloseCompressor(encoder);
        }
        const UINT payload = static_cast<UINT>(packed ? packed : size);
        // Separate temp names let multiple launcher sessions share this cache.
        std::wstring tmp(path);
        tmp += L"." + std::to_wstring(GetCurrentProcessId()) + L".tmp";
        FILE* file{};
        if (_wfopen_s(&file, tmp.c_str(), L"wb")) return;
        const bool written = fwrite(header, sizeof(header), 1, file) == 1 &&
            fwrite(&payload, sizeof(payload), 1, file) == 1 &&
            fwrite(packed ? compressed.data() : pixels.data(), 1, payload, file) == payload;
        const bool closed = fclose(file) == 0;
        if (!written || !closed || !MoveFileExW(tmp.c_str(), path, MOVEFILE_REPLACE_EXISTING))
            DeleteFileW(tmp.c_str());
    } catch (const std::bad_alloc&) {
        // Running out of cache space or memory cannot break wallpaper playback.
    }
}
