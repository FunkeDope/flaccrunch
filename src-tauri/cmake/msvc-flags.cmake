# CMake toolchain file used for the x86_64-pc-windows-msvc target.
#
# libFLAC is compiled by libflac-sys through the `cmake` crate. For the Visual
# Studio generator that crate replaces CMake's per-configuration flags with the
# flags it derived from the C compiler (which deliberately contain no
# optimization switch), so libFLAC ended up compiled at /Od in release builds.
# This file is loaded via CMAKE_TOOLCHAIN_FILE (see .cargo/config.toml) and
# forces the optimization flags back on. CMAKE_C_FLAGS (runtime library,
# /arch:AVX2 from CFLAGS) is still applied on top.
set(CMAKE_C_FLAGS_RELEASE "/O2 /Ob2 /DNDEBUG" CACHE STRING "Flags used by the C compiler during RELEASE builds." FORCE)
set(CMAKE_C_FLAGS_RELWITHDEBINFO "/O2 /Ob1 /DNDEBUG" CACHE STRING "Flags used by the C compiler during RELWITHDEBINFO builds." FORCE)
set(CMAKE_C_FLAGS_MINSIZEREL "/O1 /Ob1 /DNDEBUG" CACHE STRING "Flags used by the C compiler during MINSIZEREL builds." FORCE)
