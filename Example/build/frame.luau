--!nolint
--!nocheck
--!native
--!optimize 2

--[[
    @author SomeUser123
    @name SomeName123

    This file was automatically generated with darklua, it is not intended for manual editing.
--]]

_P.genDate = __COMPOSER.Insert(__COMPOSER.genDate)
_P.cfg = __COMPOSER.Insert(__COMPOSER.cfg)
_P.vers = __COMPOSER.Insert(__COMPOSER.vers)

--[[
    __COMPOSER is a marker for the composer to replace with actual information.

    _P is the actual runtime table that holds your build info.
    You can change _P to anything you want. But if you use luau-lsp then be sure to change the definition in .globals/pcmp.d.luau
]]

__COMPOSER.Insert(__COMPOSER.build)