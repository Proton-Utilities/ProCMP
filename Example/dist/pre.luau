--!nolint
--!nocheck
--!native
--!optimize 2

--[[
    @author SomeUser123
    @name SomeName123

    This file was automatically generated with darklua, it is not intended for manual editing.
--]]

_P.genDate = "2025-05-21T00:19:05.967076700+00:00"
_P.cfg = "Beta"
_P.vers = "v1.0.0-beta.2"

--[[
    __COMPOSER is a marker for the composer to replace with actual information.

    _P is the actual runtime table that holds your build info.
    You can change _P to anything you want. But if you use luau-lsp then be sure to change the definition in .globals/pcmp.d.luau
]]

local a a={cache={},load=function(b)if not a.cache[b]then a.cache[b]={c=a[b]()}
end return a.cache[b].c end}do function a.a()local b={}function b:Throw()print
'Thrown grenade'end function b:Destroy()print'Destroyed grenade'end return b end
end local b=a.load'a'if __BUILD=='Debug'then print
'[!] You are in a debug build.'elseif __VERSION=='Release'then getfenv(0).print=
function()return nil end getfenv(0).warn=function()return nil end getfenv(0).
print=function()return nil end end b:Throw()b:Destroy()