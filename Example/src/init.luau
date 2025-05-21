--!nocheck
--!nolint DeprecatedApi

local grenade = require("core/grenade.luau")

if _P.cfg == "Debug" then
	print("[!] You are in a debug build.")
elseif _P.cfg == "Beta" then
	print("Welcome, Beta tester!")
elseif _P.cfg == "Release" then
	getfenv(0).print = function()
		return nil
	end
	getfenv(0).warn = function()
		return nil
	end
	getfenv(0).print = function()
		return nil
	end
end

grenade:Throw()
grenade:Destroy()
