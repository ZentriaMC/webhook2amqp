MOONC = moonc
LUAJIT = luajit
USE_LUAJIT = 0

lua_files := router.lua handlers/emailengine/init.lua handlers/newreleases/init.lua
bytecode_files := $(lua_files:.lua=.ljbc) mod.ljbc lib/inspect.ljbc lib/json.ljbc lib/sha256.ljbc lib/url.ljbc lib/str.ljbc

%.lua: %.moon
	$(MOONC) -o $@ $^

%.ljbc: %.lua
	$(LUAJIT) -bg $^ $@

all: lua bytecode
bytecode: $(bytecode_files)
lua: $(lua_files)

clean:
	rm -rf $(lua_files) $(bytecode_files)
