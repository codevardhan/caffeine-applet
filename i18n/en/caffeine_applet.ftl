# Duration presets listed in the applet menu.
#
# The code decides whether a preset reads as hours or as minutes; the plural
# form is chosen here, so languages with more than two plural categories can
# add the ones they need without a code change.

duration-indefinite = Indefinitely

duration-minutes =
    { $minutes ->
        [one] { $minutes } minute
       *[other] { $minutes } minutes
    }

duration-hours =
    { $hours ->
        [one] { $hours } hour
       *[other] { $hours } hours
    }

# Live state appended to whichever preset is currently running. $label is an
# already-translated preset from above; $remaining is countdown-minutes.

# An abbreviation, invariant in English. Add a selector on $minutes if yours
# inflects.
countdown-minutes = { $minutes } min

choice-active-timed = { $label } - { $remaining } left
choice-active-indefinite = { $label } - active

# Menu actions and controls.

end-session = End Caffeine session
lid-control = Keep awake with lid closed

# Inline diagnostics shown at the bottom of the menu. $error comes from the
# operating system and stays untranslated.

error-open-settings = Could not open settings: { $error }
error-load-settings = Could not load settings: { $error }. Check the settings file; existing values are still in use.
error-start-session = Could not start a Caffeine session: { $error }
error-screensaver = Session started, but the screen may still blank: { $error }
error-lid-control = Could not enable closed-lid mode: { $error }
