--!nocheck
--!nolint DeprecatedApi

local grenade = require("core/grenade.luau")

local bInfo = getfenv().__Metadata

if bInfo.Configuration == "Debug" then
    print("[!] You are in a debug build.")
elseif bInfo.Configuration == "Release" then
    getfenv(0).print = function() return nil end
    getfenv(0).warn = function() return nil end
    getfenv(0).print = function() return nil end
end

grenade:Throw()
grenade:Destroy()