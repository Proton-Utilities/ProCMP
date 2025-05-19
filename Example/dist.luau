--!nolint
--!nocheck
--!native
--!optimize 2

--[[
    @author SomeUser123
    @name SomeName123

    This file was automatically generated with darklua, it is not intended for manual editing.
--]]

getfenv(0)._MYRIAD_BUILD = {
    Configuration = "Release",
    Version = "v1.0.0",
}

local a a={cache={},load=function(b)if not a.cache[b]then a.cache[b]={c=a[b]()}
end return a.cache[b].c end}do function a.a()local b={}function b:Throw()print
'Thrown grenade'end function b:Destroy()warn'Destroyed grenade'end return b end
end local b,c=a.load'a',getfenv()._MYRIAD_BUILD if c.Configuration=='Debug'then
print'Debug build'elseif c.Configuration=='Release'then getfenv(0).print=
function()return nil end getfenv(0).warn=function()return nil end getfenv(0).
print=function()return nil end end b:Throw()task.delay(1,b.Destroy)