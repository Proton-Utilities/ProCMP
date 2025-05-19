--!nolint
--!nocheck
--!native
--!optimize 2

--[[
    @author SomeUser123
    @name SomeName123

    This file was automatically generated with darklua, it is not intended for manual editing.
--]]

_B_VERSION = "Debug"
_B_VERSION = "v0.0.0"

local a a={cache={},load=function(b)if not a.cache[b]then a.cache[b]={c=a[b]()}
end return a.cache[b].c end}do function a.a()local b={}function b:Throw()print
'Thrown grenade'end function b:Destroy()print'Destroyed grenade'end return b end
end local b=a.load'a'if __BUILD=='Debug'then print
'[!] You are in a debug build.'elseif __VERSION=='Release'then getfenv(0).print=
function()return nil end getfenv(0).warn=function()return nil end getfenv(0).
print=function()return nil end end b:Throw()b:Destroy()