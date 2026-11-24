package cz.jancejka.launchertagssync

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import cz.jancejka.launchertagssync.ui.AppViewModel
import cz.jancejka.launchertagssync.ui.history.HistoryScreen
import cz.jancejka.launchertagssync.ui.home.HomeScreen
import cz.jancejka.launchertagssync.ui.wizard.WizardScreen

@Composable
fun MainNavigation(vm: AppViewModel) {
  val backStack = rememberNavBackStack(Home)
  val state by vm.state.collectAsStateWithLifecycle()
  LaunchedEffect(state.openWizardRequests) {
    if (state.openWizardRequests > 0 && backStack.lastOrNull() != Wizard) backStack.add(Wizard)
  }

  NavDisplay(
    backStack = backStack,
    onBack = { if (backStack.lastOrNull() == Wizard) vm.back() else backStack.removeLastOrNull() },
    entryProvider =
      entryProvider {
        entry<Home> {
          HomeScreen(
            state = state,
            onTwoWay = vm::setTwoWay,
            onStart = {
              vm.startWizard()
              backStack.add(Wizard)
            },
            onHistory = { backStack.add(History) },
            onReset = vm::resetSyncState,
          )
        }
        entry<Wizard> { WizardScreen(state, vm, onClose = { backStack.removeLastOrNull() }) }
        entry<History> { HistoryScreen(state.history, onBack = { backStack.removeLastOrNull() }) }
      },
  )
}
