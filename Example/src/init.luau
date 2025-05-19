--!nocheck
--!nolint DeprecatedApi

local grenade = require("core/grenade.luau")

if __BUILD == "Debug" then
    print("[!] You are in a debug build.")
elseif __VERSION == "Release" then
    getfenv(0).print = function() return nil end
    getfenv(0).warn = function() return nil end
    getfenv(0).print = function() return nil end
end

grenade:Throw()
grenade:Destroy()