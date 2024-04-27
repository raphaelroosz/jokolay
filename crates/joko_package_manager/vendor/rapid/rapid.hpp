#pragma once
#include "joko_package_manager/src/lib.rs.h"
#include "rust/cxx.h"

namespace rapid {
    rust::String rapid_filter(rust::String src_xml);
}