--!nocheck
--!nolint DeprecatedApi

--// Imports
local grenade = require("core/grenade.luau")

--// References
local buildInfo = getfenv()._MYRIAD_BUILD

--// Initiailize
if buildInfo.Configuration == "Debug" then
    print("Debug build")
elseif buildInfo.Configuration == "Release" then
    getfenv(0).print = function() return nil end
    getfenv(0).warn = function() return nil end
    getfenv(0).print = function() return nil end
end

grenade:Throw()
task.delay(1, grenade.Destroy)