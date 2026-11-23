package cz.jancejka.launchertagssync

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable

@Serializable data object Home : NavKey

@Serializable data object Wizard : NavKey

@Serializable data object History : NavKey
