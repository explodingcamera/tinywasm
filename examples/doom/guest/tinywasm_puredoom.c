#define DOOM_IMPLEMENTATION
#include "PureDOOM.h"

#if defined(__clang__)
#define IMPORT(name) __attribute__((import_module("env"), import_name(name)))
#else
#define IMPORT(name)
#endif

extern int host_open(const char *filename, const char *mode) IMPORT("host_open");
extern void host_close(int handle) IMPORT("host_close");
extern int host_read(int handle, void *buf, int count) IMPORT("host_read");
extern int host_write(int handle, const void *buf, int count) IMPORT("host_write");
extern int host_seek(int handle, int offset, int origin) IMPORT("host_seek");
extern int host_tell(int handle) IMPORT("host_tell");
extern int host_eof(int handle) IMPORT("host_eof");
extern void host_gettime(int *sec, int *usec) IMPORT("host_gettime");
extern void host_exit(int code) IMPORT("host_exit");
extern void host_print(const char *text) IMPORT("host_print");

extern unsigned char __heap_base;

static char g_wad_path[1024];
static char g_home_dir[] = ".";
static char g_program_name[] = "puredoom";
static char g_iwad_flag[] = "-iwad";
static char *g_argv[] = {g_program_name, g_iwad_flag, g_wad_path, 0};
static unsigned int g_heap_ptr;

static unsigned int align_up(unsigned int value, unsigned int alignment)
{
    return (value + alignment - 1u) & ~(alignment - 1u);
}

unsigned int strlen(const char *str)
{
    unsigned int len = 0;
    while (str[len] != '\0')
        ++len;
    return len;
}

static int str_eq(const char *left, const char *right)
{
    unsigned int i = 0;
    for (;;)
    {
        if (left[i] != right[i])
            return 0;
        if (left[i] == '\0')
            return 1;
        ++i;
    }
}

static void *guest_malloc(int size)
{
    unsigned int alloc_size;
    unsigned int end;
    unsigned int capacity;
    unsigned int extra_pages;
    void *result;

    if (size <= 0)
        return 0;

    if (g_heap_ptr == 0)
        g_heap_ptr = align_up((unsigned int)(unsigned long)&__heap_base, 16u);

    alloc_size = align_up((unsigned int)size, 16u);
    end = g_heap_ptr + alloc_size;
    capacity = __builtin_wasm_memory_size(0) * 65536u;

    if (end > capacity)
    {
        extra_pages = (end - capacity + 65535u) / 65536u;
        if (__builtin_wasm_memory_grow(0, extra_pages) == (unsigned int)-1)
            return 0;
    }

    result = (void *)(unsigned long)g_heap_ptr;
    g_heap_ptr = end;
    return result;
}

static void guest_free(void *ptr)
{
    (void)ptr;
}

static void *guest_open(const char *filename, const char *mode)
{
    int handle = host_open(filename, mode);
    if (handle < 0)
        return 0;
    return (void *)(unsigned long)(handle + 1);
}

static int guest_handle(void *handle)
{
    return (int)(unsigned long)handle - 1;
}

static void guest_close(void *handle)
{
    host_close(guest_handle(handle));
}

static int guest_read(void *handle, void *buf, int count)
{
    return host_read(guest_handle(handle), buf, count);
}

static int guest_write(void *handle, const void *buf, int count)
{
    return host_write(guest_handle(handle), buf, count);
}

static int guest_seek(void *handle, int offset, doom_seek_t origin)
{
    return host_seek(guest_handle(handle), offset, (int)origin);
}

static int guest_tell(void *handle)
{
    return host_tell(guest_handle(handle));
}

static int guest_eof(void *handle)
{
    return host_eof(guest_handle(handle));
}

static void guest_gettime(int *sec, int *usec)
{
    host_gettime(sec, usec);
}

static void guest_exit(int code)
{
    host_exit(code);
}

static char *guest_getenv(const char *var)
{
    if (str_eq(var, "HOME"))
        return g_home_dir;
    return 0;
}

unsigned int tinywasm_doom_wad_path_buf(void)
{
    return (unsigned int)(unsigned long)g_wad_path;
}

void tinywasm_doom_init(void)
{
    doom_set_print(host_print);
    doom_set_malloc(guest_malloc, guest_free);
    doom_set_file_io(guest_open, guest_close, guest_read, guest_write, guest_seek, guest_tell, guest_eof);
    doom_set_gettime(guest_gettime);
    doom_set_exit(guest_exit);
    doom_set_getenv(guest_getenv);

    doom_set_default_int("key_up", DOOM_KEY_W);
    doom_set_default_int("key_down", DOOM_KEY_S);
    doom_set_default_int("key_strafeleft", DOOM_KEY_A);
    doom_set_default_int("key_straferight", DOOM_KEY_D);
    doom_set_default_int("key_use", DOOM_KEY_E);
    doom_set_resolution(320, 200);

    doom_init(3, g_argv, DOOM_FLAG_HIDE_MOUSE_OPTIONS | DOOM_FLAG_HIDE_SOUND_OPTIONS | DOOM_FLAG_HIDE_MUSIC_OPTIONS);
}

void tinywasm_doom_update(void)
{
    doom_update();
}

unsigned int tinywasm_doom_framebuffer(void)
{
    return (unsigned int)(unsigned long)doom_get_framebuffer(4);
}

unsigned int tinywasm_doom_sound_buffer(void)
{
    return (unsigned int)(unsigned long)doom_get_sound_buffer();
}

unsigned long tinywasm_doom_tick_midi(void)
{
    return doom_tick_midi();
}

void tinywasm_doom_key_down(int key)
{
    doom_key_down((doom_key_t)key);
}

void tinywasm_doom_key_up(int key)
{
    doom_key_up((doom_key_t)key);
}
