cmake_minimum_required(VERSION 3.16)
project(exact_routing_osrm LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(BUILD_ROUTED OFF CACHE BOOL "HTTP server is not used" FORCE)
if(NOT OSRM_SOURCE)
  message(FATAL_ERROR "OSRM_SOURCE must be the pinned OSRM checkout")
endif()
add_subdirectory(${OSRM_SOURCE} upstream)
add_executable(osrm-bench osrm.cpp)
target_include_directories(osrm-bench PRIVATE
  ${OSRM_SOURCE}/include
  ${OSRM_SOURCE}/include/osrm
  ${CMAKE_CURRENT_BINARY_DIR}/upstream/include
  ${OSRM_SOURCE}/third_party/variant/include
  ${OSRM_SOURCE}/third_party/flatbuffers/include
  ${OSRM_SOURCE}/third_party/rapidjson/include
  ${OSRM_SOURCE}/third_party/cheap-ruler-cpp-2778eb8/include)
target_link_libraries(osrm-bench PRIVATE osrm)
target_compile_definitions(osrm-bench PRIVATE OSRM_COMMIT="4f3ee609ec1af40eb1f445c6706cfa5beb04c990")
