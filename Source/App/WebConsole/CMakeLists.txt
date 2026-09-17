project(WebConsole)

add_executable(WebConsole Main.cpp)

target_include_directories(WebConsole PRIVATE
    ${KAI_SOURCE_DIR}/Library/Core/Include
    ${KAI_SOURCE_DIR}/Library/Executor/Include
    ${KAI_SOURCE_DIR}/Library/Language/Include
    ${KAI_SOURCE_DIR}/Library/Console/Include
    ${KAI_SOURCE_DIR}/Library/Network/Include
)

target_link_libraries(WebConsole PRIVATE
    KAI_Console
    KAI_Language
    KAI_Executor
    KAI_Core
)

# Windows needs Winsock
if(WIN32)
    target_link_libraries(WebConsole PRIVATE ws2_32)
endif()

set_target_properties(WebConsole PROPERTIES
    CXX_STANDARD 23
    CXX_STANDARD_REQUIRED ON
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
)
