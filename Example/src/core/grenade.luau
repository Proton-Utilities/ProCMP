local grenade = {}

function grenade:Throw()
    print("Thrown grenade")
end

function grenade:Destroy()
    warn("Destroyed grenade")
end

return grenade